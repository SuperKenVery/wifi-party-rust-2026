//! Application state and configuration.
//!
//! This module contains shared state types used across the application:
//!
//! - [`AppState`] - Global application state (configs, connection status, etc.)
//! - [`HostId`] / [`HostInfo`] - Remote peer identification and metadata
//! - [`AudioConfig`] / [`NetworkConfig`] - Configuration structures

use crate::pipeline::graph::{PipelineGraph, Inspectable};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

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

/// Information about a remote host
#[derive(Debug, Clone)]
pub struct HostInfo {
    pub id: HostId,
    pub volume: f32,      // 0.0 to 2.0 (0-200%)
    pub audio_level: f32, // 0.0 to 1.0
    pub packet_loss: f32, // 0.0 to 1.0 (percentage)
    pub last_seen: std::time::Instant,
}

impl PartialEq for HostInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.volume == other.volume
            && self.audio_level == other.audio_level
            && self.packet_loss == other.packet_loss
    }
}

impl HostInfo {
    pub fn new(id: HostId) -> Self {
        Self {
            id,
            volume: 1.0,
            audio_level: 0.0,
            packet_loss: 0.0,
            last_seen: std::time::Instant::now(),
        }
    }
}

/// Audio device configuration
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: u32,
    pub channels: u8,
    pub frame_size: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            sample_rate: 48000,
            channels: 2,
            frame_size: 480, // 10ms at 48kHz
        }
    }
}

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub multicast_addr: String,
    pub port: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            multicast_addr: "242.355.43.2".to_string(),
            port: 7667,
        }
    }
}

/// Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connected,
}

/// Shared application state
pub struct AppState {
    pub audio_config: Arc<Mutex<AudioConfig>>,
    pub network_config: Arc<Mutex<NetworkConfig>>,
    pub connection_status: Arc<Mutex<ConnectionStatus>>,
    pub mic_muted: Arc<AtomicBool>,
    pub mic_volume: Arc<Mutex<f32>>,
    pub mic_audio_level: Arc<Mutex<f32>>,
    pub loopback_enabled: Arc<AtomicBool>,
    pub sequence_number: Arc<AtomicU64>,
    pub local_host_id: Arc<Mutex<Option<HostId>>>,
    pub host_infos: Arc<Mutex<Vec<HostInfo>>>,
    pub pipeline_graph: Arc<Mutex<PipelineGraph>>,
    // Store active pipelines for visualization
    pub pipelines: Arc<Mutex<Vec<Arc<dyn Inspectable>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            audio_config: Arc::new(Mutex::new(AudioConfig::default())),
            network_config: Arc::new(Mutex::new(NetworkConfig::default())),
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            mic_muted: Arc::new(AtomicBool::new(false)),
            mic_volume: Arc::new(Mutex::new(1.0)),
            mic_audio_level: Arc::new(Mutex::new(0.0)),
            loopback_enabled: Arc::new(AtomicBool::new(false)),
            sequence_number: Arc::new(AtomicU64::new(0)),
            local_host_id: Arc::new(Mutex::new(None)),
            host_infos: Arc::new(Mutex::new(Vec::new())),
            pipeline_graph: Arc::new(Mutex::new(PipelineGraph::new())),
            pipelines: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
