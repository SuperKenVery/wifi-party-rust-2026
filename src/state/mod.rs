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

use crate::party::{NtpDebugInfo, Party, PartyConfig, StreamSnapshot, SyncedStreamState};

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
    pub stream_id: String,
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

/// Progress state for music stream encoding and playback
pub struct MusicStreamProgress {
    pub file_name: Mutex<Option<String>>,
    pub is_encoding: AtomicBool,
    pub encoding_current: AtomicU64,
    pub encoding_total: AtomicU64,
    pub is_streaming: AtomicBool,
    pub streaming_current: AtomicU64,
    pub streaming_total: AtomicU64,
}

impl MusicStreamProgress {
    pub fn new() -> Self {
        Self {
            file_name: Mutex::new(None),
            is_encoding: AtomicBool::new(false),
            encoding_current: AtomicU64::new(0),
            encoding_total: AtomicU64::new(0),
            is_streaming: AtomicBool::new(false),
            streaming_current: AtomicU64::new(0),
            streaming_total: AtomicU64::new(0),
        }
    }

    pub fn reset(&self) {
        *self.file_name.lock().unwrap() = None;
        self.is_encoding
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.encoding_current
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.encoding_total
            .store(0, std::sync::atomic::Ordering::Relaxed);
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
    pub host_infos: Arc<Mutex<Vec<HostInfo>>>,
    pub music_progress: Arc<MusicStreamProgress>,
    pub party: Mutex<Option<Party<f32, 2, 48000>>>,
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
            host_infos: Arc::new(Mutex::new(Vec::new())),
            music_progress: Arc::new(MusicStreamProgress::new()),
            party: Mutex::new(None),
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

    pub fn stream_snapshots(&self, host_id: HostId, stream_id: &str) -> Vec<StreamSnapshot> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .map(|party| party.stream_snapshots(host_id, stream_id))
            .unwrap_or_default()
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

    pub fn synced_stream_states(&self) -> Vec<SyncedStreamState> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .map(|party| party.synced_stream_states())
            .unwrap_or_default()
    }

    pub fn ntp_debug_info(&self) -> Option<NtpDebugInfo> {
        self.party
            .lock()
            .expect("Party lock poisoned")
            .as_ref()
            .and_then(|party| party.ntp_service().map(|ntp| ntp.debug_info()))
    }
}
