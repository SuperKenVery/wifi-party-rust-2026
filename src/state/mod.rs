//! Application state and configuration.
//!
//! This module contains shared state types used across the application:
//!
//! - [`AppState`] - Global application state (configs, connection status, etc.)
//! - [`HostId`] / [`HostInfo`] - Remote peer identification and metadata

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use crate::party::{Party, PartyConfig};

/// Unique identifier for a remote host, derived from their socket address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostId(SocketAddr);

impl HostId {
    pub fn new(addr: SocketAddr) -> Self {
        Self(addr)
    }

    pub fn as_socket_addr(&self) -> &SocketAddr {
        &self.0
    }

    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl From<SocketAddr> for HostId {
    fn from(addr: SocketAddr) -> Self {
        Self(addr)
    }
}

/// Information about a single audio stream from a remote host.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamInfo {
    pub stream_id: String,
    pub audio_level: f32,
}

/// Information about a remote host
#[derive(Debug, Clone, PartialEq)]
pub struct HostInfo {
    pub id: HostId,
    pub streams: Vec<StreamInfo>,
    pub packet_loss: f32,
    pub jitter_latency_ms: f32,
    pub hardware_latency_ms: f32,
}

/// Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connected,
}

/// Shared application state
pub struct AppState {
    pub connection_status: Arc<Mutex<ConnectionStatus>>,
    pub mic_enabled: Arc<AtomicBool>,
    pub mic_volume: Arc<Mutex<f32>>,
    pub mic_audio_level: Arc<AtomicU32>,
    pub loopback_enabled: Arc<AtomicBool>,
    pub system_audio_enabled: Arc<AtomicBool>,
    pub system_audio_level: Arc<AtomicU32>,
    pub host_infos: Arc<Mutex<Vec<HostInfo>>>,
    pub party: Mutex<Option<Party<f32, 2, 48000>>>,
}

impl AppState {
    pub fn new(config: PartyConfig) -> Result<Arc<Self>> {
        let state = Arc::new(Self {
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            mic_enabled: Arc::new(AtomicBool::new(false)),
            mic_volume: Arc::new(Mutex::new(1.0)),
            mic_audio_level: Arc::new(AtomicU32::new(0)),
            loopback_enabled: Arc::new(AtomicBool::new(true)),
            system_audio_enabled: Arc::new(AtomicBool::new(false)),
            system_audio_level: Arc::new(AtomicU32::new(0)),
            host_infos: Arc::new(Mutex::new(Vec::new())),
            party: Mutex::new(None),
        });

        let mut party = Party::new(state.clone(), config);
        party.run()?;
        *state.party.lock().unwrap() = Some(party);

        Ok(state)
    }
}
