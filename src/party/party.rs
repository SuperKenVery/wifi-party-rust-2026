//! Main party orchestrator.
//!
//! Coordinates audio capture, network transport, and playback into a complete
//! audio sharing pipeline.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use tracing::{error, info};

use crate::audio::effects::Switch;
use crate::audio::{AudioBatcher, AudioSample, Gain, LevelMeter, OpusEncoder, SimpleBuffer};
use crate::io::{AudioInput, AudioOutput, LoopbackInput, NetworkSender};
use crate::pipeline::Pushable;
use crate::state::{AppState, HostId, HostInfo, MusicStreamProgress, StreamInfo};
use crate::{pull_chain, push_chain};

use super::combinator::{Mixer, Tee};
use super::config::PartyConfig;
use super::music::MusicStream;
use super::network::NetworkNode;
use super::ntp::NtpService;
use super::stream::{RealtimeAudioStream, RealtimeFramePacker, RealtimeStreamId, StreamSnapshot};
use super::sync_stream::{SyncedAudioStreamManager, SyncedStreamState};

pub struct Party<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
    config: PartyConfig,
    network_node: NetworkNode<Sample, CHANNELS, SAMPLE_RATE>,
    realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    synced_stream: Option<Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>>,
    ntp_service: Option<Arc<NtpService>>,
    network_sender: Option<NetworkSender>,
    music_streams: Mutex<Vec<MusicStream>>,
    mic_input: Option<Arc<AudioInput<Sample, CHANNELS, SAMPLE_RATE>>>,
    _audio_streams: Vec<cpal::Stream>,
    host_sync_shutdown: Arc<AtomicBool>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(state: Arc<AppState>, config: PartyConfig) -> Self {
        Self {
            state,
            config,
            network_node: NetworkNode::new(),
            realtime_stream: Arc::new(RealtimeAudioStream::new()),
            synced_stream: None,
            ntp_service: None,
            network_sender: None,
            music_streams: Mutex::new(Vec::new()),
            mic_input: None,
            _audio_streams: Vec::new(),
            host_sync_shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn mic_input(&self) -> Option<&Arc<AudioInput<Sample, CHANNELS, SAMPLE_RATE>>> {
        self.mic_input.as_ref()
    }

    pub fn stream_snapshots(&self, host_id: HostId, stream_id: &str) -> Vec<StreamSnapshot> {
        self.realtime_stream.stream_snapshots(host_id, stream_id)
    }

    pub fn synced_stream_states(&self) -> Vec<SyncedStreamState> {
        self.synced_stream
            .as_ref()
            .map(|s| s.active_streams())
            .unwrap_or_default()
    }

    pub fn ntp_service(&self) -> Option<&Arc<NtpService>> {
        self.ntp_service.as_ref()
    }

    pub fn handle_retransmission_request(
        &self,
        stream_id: super::sync_stream::SyncedStreamId,
        seqs: Vec<u64>,
    ) {
        let music_streams = self.music_streams.lock().unwrap();
        for stream in music_streams.iter() {
            if stream.stream_id() == stream_id {
                stream.handle_retransmission_request(seqs);
                break;
            }
        }
    }

    pub fn pause_music(&self, stream_id: super::sync_stream::SyncedStreamId) -> Result<()> {
        let music_streams = self.music_streams.lock().unwrap();
        for stream in music_streams.iter() {
            if stream.stream_id() == stream_id {
                stream.pause()?;
                return Ok(());
            }
        }
        anyhow::bail!("Stream not found")
    }

    pub fn resume_music(&self, stream_id: super::sync_stream::SyncedStreamId) -> Result<()> {
        let music_streams = self.music_streams.lock().unwrap();
        for stream in music_streams.iter() {
            if stream.stream_id() == stream_id {
                stream.resume()?;
                return Ok(());
            }
        }
        anyhow::bail!("Stream not found")
    }

    pub fn seek_music(
        &self,
        stream_id: super::sync_stream::SyncedStreamId,
        position_ms: u64,
    ) -> Result<()> {
        let music_streams = self.music_streams.lock().unwrap();
        for stream in music_streams.iter() {
            if stream.stream_id() == stream_id {
                stream.seek(position_ms)?;
                return Ok(());
            }
        }
        anyhow::bail!("Stream not found")
    }
}

impl<Sample: AudioSample + Clone + cpal::SizedSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn run(&mut self) -> Result<()> {
        info!("Starting Party pipelines with config {:#?}", self.config);

        let (network_sink, realtime_stream, synced_stream, ntp_service) = self.network_node.start(
            self.realtime_stream.clone(),
            self.state.clone(),
            self.config.ipv6,
            self.config.send_interface_index,
        )?;

        self.ntp_service = Some(ntp_service);
        self.synced_stream = Some(synced_stream.clone());
        self.network_sender = Some(network_sink.clone());

        let loopback_buffer = Arc::new(SimpleBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let network_sink_arc: Arc<dyn Pushable<_>> = Arc::new(network_sink);

        // ============================================================
        // Mic Pipeline: Mic -> LevelMeter -> Gain -> Tee
        //   -> AudioBatcher -> OpusEncoder -> RealtimeFramePacker(Mic) -> NetworkSink
        //   -> LoopbackSwitch -> loopback_buffer
        // ============================================================
        let mic_pipeline = push_chain![
            LevelMeter::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.mic_audio_level.clone()),
            Gain::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.mic_volume.clone()),
            => Arc::new(Tee::new(
                push_chain![
                    AudioBatcher::<Sample, CHANNELS, SAMPLE_RATE>::new(20),
                    OpusEncoder::<Sample, CHANNELS, SAMPLE_RATE>::new()?,
                    RealtimeFramePacker::new(RealtimeStreamId::Mic),
                    => network_sink_arc.clone()
                ],
                push_chain![
                    Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.loopback_enabled.clone()),
                    => loopback_buffer.clone()
                ]
            ))
        ];

        self.mic_input = Some(Arc::new(AudioInput::new(
            mic_pipeline,
            self.config.input_device_id.clone(),
        )));

        // ============================================================
        // System Audio Pipeline: SystemAudio -> LevelMeter -> SystemSwitch -> AudioBatcher -> OpusEncoder -> RealtimeFramePacker(System) -> NetworkSink
        // ============================================================
        let system_pipeline = push_chain![
            LevelMeter::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.system_audio_level.clone()),
            Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.system_audio_enabled.clone()),
            AudioBatcher::<Sample, CHANNELS, SAMPLE_RATE>::new(10),
            OpusEncoder::<Sample, CHANNELS, SAMPLE_RATE>::new()?,
            RealtimeFramePacker::new(RealtimeStreamId::System),
            => network_sink_arc.clone()
        ];

        let system_stream_result =
            LoopbackInput::new(system_pipeline).start(self.config.output_device_id.as_ref());
        let system_stream = match system_stream_result {
            Ok(stream) => Some(stream),
            Err(e) => {
                error!(
                    "Failed to start system audio capture: {}. System audio sharing will be disabled.",
                    e
                );
                None
            }
        };

        // ============================================================
        // Output Pipeline: RealtimeAudioStream (mixed from all hosts/streams)
        //                  + SyncedAudioStream (music) + loopback_buffer -> Speaker
        // listen_enabled controls whether network audio (realtime + synced) is played
        // ============================================================
        let output_mixer = Mixer::with_inputs([
            pull_chain![
                realtime_stream.mixer().clone() =>,
                Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.listen_enabled.clone())
            ],
            pull_chain![
                synced_stream.clone() =>,
                Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.listen_enabled.clone())
            ],
            loopback_buffer.clone(),
        ]);

        let audio_output = AudioOutput::new(output_mixer);
        let output_stream = audio_output.start(self.config.output_device_id.as_ref())?;

        let mut streams = vec![output_stream];
        if let Some(sys_stream) = system_stream {
            streams.push(sys_stream);
        }
        self._audio_streams = streams;

        self.start_host_sync_task();

        info!("Party pipelines configured successfully");

        Ok(())
    }

    pub fn restart_with_config(&mut self, config: PartyConfig) -> Result<()> {
        info!("Restarting Party with new config...");

        self.host_sync_shutdown.store(true, Ordering::Relaxed);

        self._audio_streams.clear();
        self.mic_input = None;
        {
            let mut music_streams = self.music_streams.lock().unwrap();
            music_streams.clear();
        }
        // Drop all NetworkSender references before shutting down NetworkNode
        self.network_sender = None;
        self.ntp_service = None;
        self.synced_stream = None;

        self.config = config;
        self.network_node = NetworkNode::new();
        self.realtime_stream = Arc::new(RealtimeAudioStream::new());
        self.host_sync_shutdown = Arc::new(AtomicBool::new(false));

        self.run()
    }

    pub fn start_music_stream(
        &self,
        data: Vec<u8>,
        file_name: String,
        progress: Arc<MusicStreamProgress>,
    ) -> Result<()> {
        let network_sender = self
            .network_sender
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Network not started"))?
            .clone();

        let ntp_service = self
            .ntp_service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("NTP service not started"))?
            .clone();

        let synced_stream = self
            .synced_stream
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Synced stream not started"))?
            .clone();

        let music_stream = MusicStream::start::<Sample, CHANNELS, SAMPLE_RATE>(
            data,
            file_name,
            ntp_service,
            network_sender,
            synced_stream,
            progress,
        )?;

        let mut music_streams = self.music_streams.lock().unwrap();
        music_streams.push(music_stream);

        Ok(())
    }

    fn start_host_sync_task(&self) {
        let state = self.state.clone();
        let realtime_stream = self.realtime_stream.clone();
        let shutdown = self.host_sync_shutdown.clone();

        thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100));

                let mut active_host_ids = realtime_stream.active_hosts();
                // Sort hosts by ID to ensure stable UI order and prevent flickering
                active_host_ids.sort_by_key(|h| h.to_string());

                let mut host_infos_vec = Vec::new();

                for host_id in active_host_ids {
                    let stream_stats = realtime_stream.host_stream_stats(host_id);

                    let streams: Vec<StreamInfo> = stream_stats
                        .into_iter()
                        .map(|s| StreamInfo {
                            stream_id: s.stream_id.to_string(),
                            packet_loss: s.packet_loss,
                            target_latency: s.target_latency,
                            audio_level: s.audio_level,
                        })
                        .collect();

                    host_infos_vec.push(HostInfo {
                        id: host_id,
                        streams,
                    });
                }

                if let Ok(mut host_infos) = state.host_infos.lock() {
                    *host_infos = host_infos_vec;
                }
            }
        });
    }
}
