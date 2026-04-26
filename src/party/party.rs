//! Main party orchestrator.
//!
//! Coordinates audio capture, network transport, and playback into a complete
//! audio sharing pipeline.

use std::net::UdpSocket;
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use tracing::{error, info};

use crate::audio::effects::Switch;
use crate::audio::{AudioBatcher, AudioSample, Gain, LevelMeter, OpusEncoder, SimpleBuffer};
use crate::io::{
    AudioInput, AudioOutput, LoopbackInput, MulticastLock, NetworkSender, create_multicast_socket,
};
use crate::pipeline::Pushable;
use crate::state::{AppState, MusicStreamProgress};
use crate::{pull_chain, push_chain};

use super::combinator::{Mixer, Tee};
use super::config::PartyConfig;
use super::music::MusicStreamRegistry;
use super::network_stream::{NetworkStream, NetworkStreamContext, StreamRegistry};
use super::ntp::NtpService;
use super::packet_dispatcher::PacketDispatcher;
use super::realtime_stream::{RealtimeAudioStream, RealtimeFramePacker, RealtimeStreamId};
use super::sync_stream::SyncedAudioStreamManager;

struct NetworkStreamBundle<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    ntp_service: Arc<NtpService>,
    synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
    registry: Arc<StreamRegistry<Sample, CHANNELS, SAMPLE_RATE>>,
}

pub struct Party<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
    config: PartyConfig,
    realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    synced_stream: Option<Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>>,
    ntp_service: Option<Arc<NtpService>>,
    music_streams: Option<Arc<MusicStreamRegistry<Sample, CHANNELS, SAMPLE_RATE>>>,
    mic_input: Option<Arc<AudioInput<Sample, CHANNELS, SAMPLE_RATE>>>,
    _audio_streams: Vec<cpal::Stream>,
    dispatcher_abort: Option<tokio::task::AbortHandle>,
    network_thread: Option<thread::JoinHandle<()>>,
    #[allow(dead_code)]
    multicast_lock: Option<MulticastLock>,
}

impl<Sample: AudioSample + 'static, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(state: Arc<AppState>, config: PartyConfig) -> Self {
        Self {
            state,
            config,
            realtime_stream: Arc::new(RealtimeAudioStream::new()),
            synced_stream: None,
            ntp_service: None,
            music_streams: None,
            mic_input: None,
            _audio_streams: Vec::new(),
            dispatcher_abort: None,
            network_thread: None,
            multicast_lock: None,
        }
    }

    pub fn mic_input(&self) -> Option<&Arc<AudioInput<Sample, CHANNELS, SAMPLE_RATE>>> {
        self.mic_input.as_ref()
    }

    pub fn pause_music(&self, stream_id: super::sync_stream::SyncedStreamId) -> Result<()> {
        self.music_streams()?.pause(stream_id)
    }

    pub fn resume_music(&self, stream_id: super::sync_stream::SyncedStreamId) -> Result<()> {
        self.music_streams()?.resume(stream_id)
    }

    pub fn seek_music(
        &self,
        stream_id: super::sync_stream::SyncedStreamId,
        position_ms: u64,
    ) -> Result<()> {
        self.music_streams()?.seek(stream_id, position_ms)
    }

    fn music_streams(&self) -> Result<&Arc<MusicStreamRegistry<Sample, CHANNELS, SAMPLE_RATE>>> {
        self.music_streams
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Music stream registry not started"))
    }
}

impl<
    Sample: AudioSample + Clone + cpal::SizedSample + 'static,
    const CHANNELS: usize,
    const SAMPLE_RATE: u32,
> Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn run(&mut self) -> Result<()> {
        info!("Starting Party pipelines with config {:#?}", self.config);

        self.multicast_lock = MulticastLock::acquire();

        let (socket, multicast_addr, local_ips) =
            create_multicast_socket(self.config.ipv6, self.config.send_interface_index)?;

        let send_socket: UdpSocket = socket
            .try_clone()
            .context("Failed to clone socket for sender")?;
        let network_sender = NetworkSender::new(send_socket, multicast_addr);

        let stream_bundle = self.build_stream_bundle(network_sender.clone());

        let realtime_stream = self.realtime_stream.clone();
        let synced_stream = stream_bundle.synced_stream.clone();
        self.ntp_service = Some(stream_bundle.ntp_service.clone());
        self.synced_stream = Some(stream_bundle.synced_stream.clone());

        let (abort_tx, abort_rx) = std::sync::mpsc::sync_channel(1);

        self.network_thread = Some(thread::spawn({
            let state = self.state.clone();
            let view_state = self.state.view_state.clone();
            let registry = stream_bundle.registry.clone();
            let network_sender = network_sender.clone();

            move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create Tokio runtime");

                rt.block_on(async {
                    registry.start_all(NetworkStreamContext {
                        view_state,
                        sender: network_sender,
                    });

                    let handle = PacketDispatcher::start(socket, local_ips, state, registry);
                    let _ = abort_tx.send(handle.abort_handle());
                    handle.await.ok();
                });
            }
        }));

        self.dispatcher_abort = Some(abort_rx.recv().expect("network thread failed to start"));

        let loopback_buffer = Arc::new(SimpleBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let network_sink_arc: Arc<dyn Pushable<_>> = Arc::new(network_sender);

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

        info!("Party pipelines configured successfully");

        Ok(())
    }

    pub fn restart_with_config(&mut self, config: PartyConfig) -> Result<()> {
        info!("Restarting Party with new config...");

        if let Some(abort) = self.dispatcher_abort.take() {
            abort.abort();
        }

        self._audio_streams.clear();
        self.mic_input = None;
        if let Some(music_streams) = self.music_streams.take() {
            music_streams.clear();
        }
        self.ntp_service = None;
        self.synced_stream = None;
        self.multicast_lock = None;
        self.state.view_state.clear();

        if let Some(handle) = self.network_thread.take() {
            let _ = handle.join();
        }

        self.config = config;
        self.realtime_stream = Arc::new(RealtimeAudioStream::new());

        self.run()
    }

    pub fn start_music_stream(
        &self,
        data: Vec<u8>,
        file_name: String,
        progress: Arc<MusicStreamProgress>,
    ) -> Result<()> {
        self.music_streams()?.start_stream(data, file_name, progress)
    }

    fn build_stream_bundle(
        &mut self,
        network_sender: NetworkSender,
    ) -> NetworkStreamBundle<Sample, CHANNELS, SAMPLE_RATE> {
        let ntp_service = NtpService::new(network_sender.clone());

        let ntp_for_synced = ntp_service.clone();
        let synced_stream = Arc::new(SyncedAudioStreamManager::new(
            move || ntp_for_synced.party_now(),
            self.state.vocal_removal_enabled.clone(),
        ));

        let music_streams = Arc::new(MusicStreamRegistry::new(
            ntp_service.clone(),
            network_sender,
            synced_stream.clone(),
        ));
        self.music_streams = Some(music_streams.clone());

        let streams: Vec<Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>> = vec![
            self.realtime_stream.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
            synced_stream.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
            ntp_service.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
            music_streams as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
        ];

        NetworkStreamBundle {
            ntp_service,
            synced_stream,
            registry: Arc::new(StreamRegistry::from_streams(streams)),
        }
    }
}
