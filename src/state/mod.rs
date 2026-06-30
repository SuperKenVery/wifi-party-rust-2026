//! Application state and configuration.
//!
//! This module contains shared state types used across the application:
//!
//! - [`AppState`] - Global application state (configs, connection status, etc.)
//! - [`HostId`] / [`HostInfo`] - Remote peer identification and metadata

use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex};

use crate::io::SendTarget;
use crate::music_provider::ProviderFactory;
use crate::party::{Party, PartyConfig};

mod view_state;

pub use view_state::{PartyViewState, StreamViewKey};

/// Unique identifier for a remote host, derived from their IP address.
/// We use IP address instead of SocketAddr to keep the host identity stable
/// even if the ephemeral source port changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostId(IpAddr);

impl HostId {
    pub fn new(addr: IpAddr) -> Self {
        Self(addr)
    }

    pub fn ip(&self) -> IpAddr {
        self.0
    }

    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl From<IpAddr> for HostId {
    fn from(addr: IpAddr) -> Self {
        Self(addr)
    }
}

impl From<SocketAddr> for HostId {
    fn from(addr: SocketAddr) -> Self {
        Self(addr.ip())
    }
}

/// Information about a single audio stream from a remote host.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamInfo {
    pub key: StreamViewKey,
    pub display_name: String,
    pub packet_loss: f32,
    pub target_latency: f32,
    pub audio_level: u32,
}

/// Information about a remote host
#[derive(Debug, Clone, PartialEq)]
pub struct HostInfo {
    pub id: HostId,
    pub streams: Vec<StreamInfo>,
}

/// Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connected,
}

/// Progress state for music stream sending and playback
pub struct MusicStreamProgress {
    pub file_name: Mutex<Option<String>>,
    pub is_streaming: AtomicBool,
    pub streaming_current: AtomicU64,
    pub streaming_total: AtomicU64,
}

impl MusicStreamProgress {
    pub fn new() -> Self {
        Self {
            file_name: Mutex::new(None),
            is_streaming: AtomicBool::new(false),
            streaming_current: AtomicU64::new(0),
            streaming_total: AtomicU64::new(0),
        }
    }

    pub fn reset(&self) {
        *self.file_name.lock().unwrap() = None;
        self.is_streaming
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.streaming_current
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.streaming_total
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Shared application state
pub struct AppState {
    pub connection_status: Arc<Mutex<ConnectionStatus>>,
    pub mic_volume: Arc<Mutex<f32>>,
    pub mic_audio_level: Arc<AtomicU32>,
    pub loopback_enabled: Arc<AtomicBool>,
    pub system_audio_enabled: Arc<AtomicBool>,
    pub system_audio_level: Arc<AtomicU32>,
    pub listen_enabled: Arc<AtomicBool>,
    pub vocal_removal_enabled: Arc<AtomicBool>,
    pub view_state: Arc<PartyViewState>,
    pub music_progress: Arc<MusicStreamProgress>,
    pub send_target: Arc<Mutex<SendTarget>>,
    pub party: Mutex<Option<Party<f32, 2, 48000>>>,
    pub music_provider_factories: &'static [ProviderFactory],
}

impl AppState {
    pub fn new(config: PartyConfig) -> Result<Arc<Self>> {
        let state = Arc::new(Self {
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            mic_volume: Arc::new(Mutex::new(1.0)),
            mic_audio_level: Arc::new(AtomicU32::new(0)),
            loopback_enabled: Arc::new(AtomicBool::new(true)),
            system_audio_enabled: Arc::new(AtomicBool::new(false)),
            system_audio_level: Arc::new(AtomicU32::new(0)),
            listen_enabled: Arc::new(AtomicBool::new(true)),
            vocal_removal_enabled: Arc::new(AtomicBool::new(false)),
            view_state: Arc::new(PartyViewState::new()),
            music_progress: Arc::new(MusicStreamProgress::new()),
            send_target: Arc::new(Mutex::new(SendTarget::Multicast)),
            party: Mutex::new(None),
            music_provider_factories: &[
                crate::music_provider::local_file::factory,
                #[cfg(feature = "music-provider-apple-music")]
                crate::music_provider::apple_music::factory,
            ],
        });

        let mut party = Party::new(state.clone(), config);
        party.run()?;
        *state.party.lock().unwrap() = Some(party);

        Ok(state)
    }

    pub fn enable_mic(&self) -> Result<()> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .mic_input()
            .context("Mic input not initialized")?
            .enable()
    }

    pub fn disable_mic(&self) {
        if let Some(party) = self.party.lock().expect("Party lock poisoned").as_ref() {
            if let Some(mic_input) = party.mic_input() {
                mic_input.disable();
            }
        }
    }

    pub fn start_music_stream(&self, data: Vec<u8>, file_name: String) -> Result<()> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .start_music_stream(data, file_name, self.music_progress.clone())
    }

    pub fn pause_music(&self, stream_id: crate::party::SyncedStreamId) -> Result<()> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .pause_music(stream_id)
    }

    pub fn resume_music(&self, stream_id: crate::party::SyncedStreamId) -> Result<()> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .resume_music(stream_id)
    }

    pub fn seek_music(
        &self,
        stream_id: crate::party::SyncedStreamId,
        position_ms: u64,
    ) -> Result<()> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .seek_music(stream_id, position_ms)
    }

    pub fn set_music_vocal_removal(
        &self,
        stream_id: crate::party::SyncedStreamId,
        enabled: bool,
    ) -> Result<()> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .set_music_vocal_removal(stream_id, enabled)
    }

    // -- Playlist operations --

    /// Add a song to the shared playlist. The audio data is cached locally
    /// and the entry is broadcast to all peers.
    pub fn playlist_add(&self, data: Vec<u8>, title: String) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.add_entry(data, title);
        Ok(())
    }

    pub fn playlist_remove(&self, entry_id: u64) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.remove_entry(entry_id);
        Ok(())
    }

    pub fn playlist_move(&self, entry_id: u64, new_index: usize) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.move_entry(entry_id, new_index);
        Ok(())
    }

    pub fn playlist_play(&self, entry_id: u64) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.set_current(Some(entry_id));
        Ok(())
    }

    pub fn playlist_skip(&self) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.skip();
        Ok(())
    }

    pub fn playlist_previous(&self) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.previous();
        Ok(())
    }

    pub fn playlist_clear(&self) -> Result<()> {
        let playlist = self.playlist_handle()?;
        playlist.clear();
        Ok(())
    }

    fn playlist_handle(&self) -> Result<Arc<crate::party::SharedPlaylist>> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .context("Party not initialized")?
            .playlist_handle()
    }

    pub fn send_target(&self) -> SendTarget {
        self.send_target
            .lock()
            .map(|target| target.clone())
            .unwrap_or_default()
    }

    pub fn set_send_target(&self, target: SendTarget) -> Result<()> {
        if let SendTarget::Unicast(ip) = target {
            let party_guard = self.party.lock().expect("Party lock poisoned");
            if let Some(party) = party_guard.as_ref() {
                let uses_ipv6 = party.uses_ipv6();
                if uses_ipv6 != ip.is_ipv6() {
                    anyhow::bail!(
                        "Target IP family does not match the active {} socket",
                        if uses_ipv6 { "IPv6" } else { "IPv4" }
                    );
                }
            }
        }

        *self.send_target.lock().expect("Send target lock poisoned") = target;
        Ok(())
    }
}
