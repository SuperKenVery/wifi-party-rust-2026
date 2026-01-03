use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

/// Host identifier using IPv4 address
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HostId([u8; 4]);

impl HostId {
    pub fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }

    pub fn to_string(&self) -> String {
        format!("{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

impl From<[u8; 4]> for HostId {
    fn from(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }
}

/// Information about a remote host
#[derive(Debug, Clone, PartialEq)]
pub struct HostInfo {
    pub id: HostId,
    pub volume: f32,      // 0.0 to 2.0 (0-200%)
    pub audio_level: f32, // 0.0 to 1.0
    pub packet_loss: f32, // 0.0 to 1.0 (percentage)
    pub last_seen: std::time::Instant,
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
    pub active_hosts: Arc<Mutex<HashMap<HostId, HostInfo>>>,
    pub connection_status: Arc<Mutex<ConnectionStatus>>,
    pub mic_muted: Arc<AtomicBool>,
    pub mic_volume: Arc<Mutex<f32>>,
    pub mic_audio_level: Arc<Mutex<f32>>,
    pub loopback_enabled: Arc<AtomicBool>,
    pub sequence_number: Arc<AtomicU64>,
    pub local_host_id: Arc<Mutex<Option<HostId>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            audio_config: Arc::new(Mutex::new(AudioConfig::default())),
            network_config: Arc::new(Mutex::new(NetworkConfig::default())),
            active_hosts: Arc::new(Mutex::new(HashMap::new())),
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            mic_muted: Arc::new(AtomicBool::new(false)),
            mic_volume: Arc::new(Mutex::new(1.0)),
            mic_audio_level: Arc::new(Mutex::new(0.0)),
            loopback_enabled: Arc::new(AtomicBool::new(false)),
            sequence_number: Arc::new(AtomicU64::new(0)),
            local_host_id: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
