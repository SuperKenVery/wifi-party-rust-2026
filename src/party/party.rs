//! Main party orchestrator.
//!
//! Coordinates audio capture, network transport, and playback into a complete
//! audio sharing pipeline.

use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use tracing::{error, info};

use crate::audio::effects::Switch;
use crate::audio::{AudioBatcher, AudioSample, Gain, LevelMeter, OpusEncoder, SimpleBuffer};
use crate::io::{
    AudioInput, AudioOutput, LoopbackInput, MulticastLock, NetworkSender, SendTarget,
    create_multicast_socket,
};
use crate::pipeline::Pushable;
use crate::state::{AppState, MusicStreamProgress};
use crate::{pull_chain, push_chain};

use super::combinator::{Mixer, Tee};
use super::config::PartyConfig;
use super::network_stream::{NetworkStream, NetworkStreamContext, StreamRegistry};
use super::ntp::NtpService;
use super::packet_dispatcher::PacketDispatcher;
use super::realtime_stream::{RealtimeAudioStream, RealtimeFramePacker, RealtimeStreamId};
use super::share_music::{ShareMusicService, SyncedStreamId, SharedPlaylist};

struct NetworkStreamBundle<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    ntp_service: Arc<NtpService>,
    share_music: Arc<ShareMusicService<Sample, CHANNELS, SAMPLE_RATE>>,
    playlist: Arc<SharedPlaylist>,
    registry: Arc<StreamRegistry<Sample, CHANNELS, SAMPLE_RATE>>,
}

pub struct Party<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
    config: PartyConfig,
    realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    share_music: Option<Arc<ShareMusicService<Sample, CHANNELS, SAMPLE_RATE>>>,
    playlist: Option<Arc<SharedPlaylist>>,
    ntp_service: Option<Arc<NtpService>>,
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
            share_music: None,
            playlist: None,
            ntp_service: None,
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

    pub fn uses_ipv6(&self) -> bool {
        self.config.ipv6
    }

    pub fn pause_music(&self, stream_id: SyncedStreamId) -> Result<()> {
        self.share_music()?.pause(stream_id)
    }

    pub fn resume_music(&self, stream_id: SyncedStreamId) -> Result<()> {
        self.share_music()?.resume(stream_id)
    }

    pub fn seek_music(&self, stream_id: SyncedStreamId, position_ms: u64) -> Result<()> {
        self.share_music()?.seek(stream_id, position_ms)
    }

    pub fn set_music_vocal_removal(&self, stream_id: SyncedStreamId, enabled: bool) -> Result<()> {
        self.share_music()?.set_vocal_removal(stream_id, enabled)
    }

    // -- Playlist delegation --

    fn playlist(&self) -> Result<&Arc<SharedPlaylist>> {
        self.playlist
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("SharedPlaylist not started"))
    }

    pub fn playlist_add(&self, data: Vec<u8>, title: String) -> Result<()> {
        self.playlist()?.add_entry(data, title);
        Ok(())
    }

    pub fn playlist_remove(&self, entry_id: u64) -> Result<()> {
        self.playlist()?.remove_entry(entry_id);
        Ok(())
    }

    pub fn playlist_move(&self, entry_id: u64, new_index: usize) -> Result<()> {
        self.playlist()?.move_entry(entry_id, new_index);
        Ok(())
    }

    pub fn playlist_play(&self, entry_id: u64) -> Result<()> {
        self.playlist()?.set_current(Some(entry_id));
        Ok(())
    }

    pub fn playlist_skip(&self) -> Result<()> {
        self.playlist()?.skip();
        Ok(())
    }

    pub fn playlist_previous(&self) -> Result<()> {
        self.playlist()?.previous();
        Ok(())
    }

    pub fn playlist_clear(&self) -> Result<()> {
        self.playlist()?.clear();
        Ok(())
    }

    fn share_music(&self) -> Result<&Arc<ShareMusicService<Sample, CHANNELS, SAMPLE_RATE>>> {
        self.share_music
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ShareMusicService not started"))
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
        self.normalize_send_target_for_config();

        let (socket, multicast_addr, local_ips) =
            create_multicast_socket(self.config.ipv6, self.config.send_interface_index)?;

        let send_socket: UdpSocket = socket
            .try_clone()
            .context("Failed to clone socket for sender")?;
        let network_sender =
            NetworkSender::new(send_socket, multicast_addr, self.state.send_target.clone());

        let stream_bundle = self.build_stream_bundle(network_sender.clone(), local_ips.clone());

        let realtime_stream = self.realtime_stream.clone();
        let synced_stream = stream_bundle.share_music.receiver();
        self.ntp_service = Some(stream_bundle.ntp_service.clone());
        self.share_music = Some(stream_bundle.share_music.clone());
        self.playlist = Some(stream_bundle.playlist.clone());

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
        if let Some(share_music) = self.share_music.take() {
            share_music.clear();
        }
        if let Some(playlist) = self.playlist.take() {
            playlist.clear();
        }
        self.ntp_service = None;
        self.share_music = None;
        self.playlist = None;
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
        self.share_music()?.start_stream(data, file_name, progress)
    }

    fn build_stream_bundle(
        &mut self,
        network_sender: NetworkSender,
        local_ips: Vec<IpAddr>,
    ) -> NetworkStreamBundle<Sample, CHANNELS, SAMPLE_RATE> {
        let ntp_service = NtpService::new(network_sender.clone());

        let ntp_for_synced = ntp_service.clone();
        let share_music = Arc::new(ShareMusicService::new(
            ntp_service.clone(),
            network_sender.clone(),
            move || ntp_for_synced.party_now(),
            self.state.vocal_removal_enabled.clone(),
        ));

        let ntp_for_playlist = ntp_service.clone();
        let playlist = Arc::new(SharedPlaylist::new(
            Arc::downgrade(&self.state),
            network_sender,
            local_ips,
            self.state.view_state.clone(),
            move || ntp_for_playlist.party_now(),
        ));

        let streams: Vec<Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>> = vec![
            self.realtime_stream.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
            share_music.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
            ntp_service.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
            playlist.clone() as Arc<dyn NetworkStream<Sample, CHANNELS, SAMPLE_RATE>>,
        ];

        NetworkStreamBundle {
            ntp_service,
            share_music,
            playlist,
            registry: Arc::new(StreamRegistry::from_streams(streams)),
        }
    }

    fn normalize_send_target_for_config(&self) {
        let Ok(mut send_target) = self.state.send_target.lock() else {
            return;
        };

        let SendTarget::Unicast(ip) = &*send_target else {
            return;
        };
        let ip = *ip;

        if ip.is_ipv6() != self.config.ipv6 {
            info!(
                "Resetting incompatible unicast target {} after switching to {}",
                ip,
                if self.config.ipv6 { "IPv6" } else { "IPv4" }
            );
            *send_target = SendTarget::Multicast;
        }
    }
}
