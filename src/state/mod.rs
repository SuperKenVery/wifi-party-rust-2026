use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use crate::audio::jitter::HostJitterBuffer;
use tokio::sync::broadcast;

pub struct JitterBufferMap {
    buffers: Mutex<HashMap<HostId, HostJitterBuffer>>,
}

impl JitterBufferMap {
    pub fn new() -> Self {
        Self {
            buffers: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_or_create(&self, host_id: HostId) -> Option<()> {
        let mut buffers = self.buffers.lock().unwrap();
        if !buffers.contains_key(&host_id) {
            let buffer = HostJitterBuffer::new(48000, 2);
            buffers.insert(host_id, buffer);
        }
        Some(())
    }

    pub fn push_frame(&self, host_id: HostId, frame: crate::audio::AudioFrame) {
        let mut buffers = self.buffers.lock().unwrap();
        if let Some(buffer) = buffers.get_mut(&host_id) {
            buffer.push(frame);
        }
    }

    pub fn pop_frame(&self, host_id: HostId) -> Option<Vec<i16>> {
        let mut buffers = self.buffers.lock().unwrap();
        if let Some(buffer) = buffers.get_mut(&host_id) {
            buffer.pop()
        } else {
            None
        }
    }

    pub fn remove(&self, host_id: &HostId) {
        let mut buffers = self.buffers.lock().unwrap();
        buffers.remove(host_id);
    }
}

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

/// State update notifications for reactive UI
#[derive(Debug, Clone)]
pub enum StateUpdate {
    ConnectionStatusChanged(ConnectionStatus),
    ActiveHostsChanged(Vec<HostInfo>),
    MicMutedChanged(bool),
    MicVolumeChanged(f32),
    MicAudioLevelChanged(f32),
    LoopbackEnabledChanged(bool),
    LocalHostIdChanged(String),
}

/// Shared application state
pub struct AppState {
    pub audio_config: Arc<Mutex<AudioConfig>>,
    pub network_config: Arc<Mutex<NetworkConfig>>,
    pub active_hosts: Arc<Mutex<HashMap<HostId, HostInfo>>>,
    pub jitter_buffers: Arc<JitterBufferMap>,
    pub connection_status: Arc<Mutex<ConnectionStatus>>,
    pub mic_muted: Arc<AtomicBool>,
    pub mic_volume: Arc<Mutex<f32>>,
    pub mic_audio_level: Arc<Mutex<f32>>,
    pub loopback_enabled: Arc<AtomicBool>,
    pub sequence_number: Arc<AtomicU64>,
    pub local_host_id: Arc<Mutex<Option<HostId>>>,
    // Reactive state update channel (using tokio broadcast for multiple receivers)
    pub state_update_tx: broadcast::Sender<StateUpdate>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000); // Buffer up to 1000 messages
        Self {
            audio_config: Arc::new(Mutex::new(AudioConfig::default())),
            network_config: Arc::new(Mutex::new(NetworkConfig::default())),
            active_hosts: Arc::new(Mutex::new(HashMap::new())),
            jitter_buffers: Arc::new(JitterBufferMap::new()),
            connection_status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            mic_muted: Arc::new(AtomicBool::new(false)),
            mic_volume: Arc::new(Mutex::new(1.0)),
            mic_audio_level: Arc::new(Mutex::new(0.0)),
            loopback_enabled: Arc::new(AtomicBool::new(false)),
            sequence_number: Arc::new(AtomicU64::new(0)),
            local_host_id: Arc::new(Mutex::new(None)),
            state_update_tx: tx,
        }
    }
    
    /// Subscribe to state updates (returns a receiver that can be used in async contexts)
    pub fn subscribe(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_update_tx.subscribe()
    }

    /// Helper method to update connection status and notify UI
    pub fn set_connection_status(&self, status: ConnectionStatus) {
        *self.connection_status.lock().unwrap() = status;
        let _ = self.state_update_tx.send(StateUpdate::ConnectionStatusChanged(status));
    }

    /// Helper method to update active hosts and notify UI
    pub fn update_active_hosts(&self) {
        let hosts: Vec<HostInfo> = self
            .active_hosts
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect();
        let _ = self.state_update_tx.send(StateUpdate::ActiveHostsChanged(hosts));
    }

    /// Helper method to update mic muted and notify UI
    pub fn set_mic_muted(&self, muted: bool) {
        self.mic_muted.store(muted, std::sync::atomic::Ordering::Relaxed);
        let _ = self.state_update_tx.send(StateUpdate::MicMutedChanged(muted));
    }

    /// Helper method to update mic volume and notify UI
    pub fn set_mic_volume(&self, volume: f32) {
        *self.mic_volume.lock().unwrap() = volume;
        let _ = self.state_update_tx.send(StateUpdate::MicVolumeChanged(volume));
    }

    /// Helper method to update mic audio level and notify UI
    pub fn set_mic_audio_level(&self, level: f32) {
        *self.mic_audio_level.lock().unwrap() = level;
        let _ = self.state_update_tx.send(StateUpdate::MicAudioLevelChanged(level));
    }

    /// Helper method to update loopback enabled and notify UI
    pub fn set_loopback_enabled(&self, enabled: bool) {
        self.loopback_enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        let _ = self.state_update_tx.send(StateUpdate::LoopbackEnabledChanged(enabled));
    }

    /// Helper method to update local host ID and notify UI
    pub fn set_local_host_id(&self, id: Option<HostId>) {
        *self.local_host_id.lock().unwrap() = id;
        if let Some(id) = id {
            let _ = self.state_update_tx.send(StateUpdate::LocalHostIdChanged(id.to_string()));
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
