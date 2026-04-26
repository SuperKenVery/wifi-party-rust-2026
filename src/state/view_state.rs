use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use crate::party::{NtpDebugInfo, StreamSnapshot, SyncedStreamState};
use crate::state::{HostId, HostInfo, StreamInfo};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamViewKey {
    pub host_id: HostId,
    pub source_addr: SocketAddr,
    pub stream_id: String,
}

pub struct RealtimeStreamView {
    pub display_name: Arc<str>,
    packet_loss_ppm: AtomicU32,
    target_latency_frames: AtomicU32,
    audio_level: AtomicU32,
    graph: Mutex<Vec<StreamSnapshot>>,
}

impl RealtimeStreamView {
    fn new(_key: &StreamViewKey, display_name: String) -> Self {
        Self {
            display_name: Arc::from(display_name),
            packet_loss_ppm: AtomicU32::new(0),
            target_latency_frames: AtomicU32::new(0),
            audio_level: AtomicU32::new(0),
            graph: Mutex::new(Vec::new()),
        }
    }

    pub fn update(
        &self,
        packet_loss: f32,
        target_latency_frames: u32,
        audio_level: u32,
        graph: Vec<StreamSnapshot>,
    ) {
        let packet_loss_ppm = (packet_loss.clamp(0.0, 1.0) * 1_000_000.0) as u32;
        self.packet_loss_ppm
            .store(packet_loss_ppm, Ordering::Relaxed);
        self.target_latency_frames
            .store(target_latency_frames, Ordering::Relaxed);
        self.audio_level.store(audio_level, Ordering::Relaxed);

        if let Ok(mut snapshots) = self.graph.lock() {
            *snapshots = graph;
        }
    }

    fn stream_info(&self, key: StreamViewKey) -> StreamInfo {
        StreamInfo {
            key,
            display_name: self.display_name.to_string(),
            packet_loss: self.packet_loss_ppm.load(Ordering::Relaxed) as f32 / 1_000_000.0,
            target_latency: self.target_latency_frames.load(Ordering::Relaxed) as f32,
            audio_level: self.audio_level.load(Ordering::Relaxed),
        }
    }

    fn graph(&self) -> Vec<StreamSnapshot> {
        self.graph.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

pub struct NtpView {
    available: AtomicBool,
    synced: AtomicBool,
    offset_micros: AtomicI64,
    local_time_micros: AtomicU64,
    party_time_micros: AtomicU64,
    pending_requests: AtomicU32,
    pending_responses: AtomicU32,
    party_time_formatted: Mutex<String>,
}

impl NtpView {
    fn new() -> Self {
        Self {
            available: AtomicBool::new(false),
            synced: AtomicBool::new(false),
            offset_micros: AtomicI64::new(0),
            local_time_micros: AtomicU64::new(0),
            party_time_micros: AtomicU64::new(0),
            pending_requests: AtomicU32::new(0),
            pending_responses: AtomicU32::new(0),
            party_time_formatted: Mutex::new(String::new()),
        }
    }

    pub fn update(&self, info: NtpDebugInfo) {
        self.synced.store(info.synced, Ordering::Relaxed);
        self.offset_micros
            .store(info.offset_micros, Ordering::Relaxed);
        self.local_time_micros
            .store(info.local_time_micros, Ordering::Relaxed);
        self.party_time_micros
            .store(info.party_time_micros, Ordering::Relaxed);
        self.pending_requests
            .store(info.pending_requests as u32, Ordering::Relaxed);
        self.pending_responses
            .store(info.pending_responses as u32, Ordering::Relaxed);
        if let Ok(mut formatted) = self.party_time_formatted.lock() {
            *formatted = info.party_time_formatted;
        }
        self.available.store(true, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Option<NtpDebugInfo> {
        if !self.available.load(Ordering::Relaxed) {
            return None;
        }

        Some(NtpDebugInfo {
            synced: self.synced.load(Ordering::Relaxed),
            offset_micros: self.offset_micros.load(Ordering::Relaxed),
            local_time_micros: self.local_time_micros.load(Ordering::Relaxed),
            party_time_micros: self.party_time_micros.load(Ordering::Relaxed),
            party_time_formatted: self
                .party_time_formatted
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default(),
            pending_requests: self.pending_requests.load(Ordering::Relaxed) as usize,
            pending_responses: self.pending_responses.load(Ordering::Relaxed) as usize,
        })
    }
}

pub struct PartyViewState {
    realtime_streams: DashMap<StreamViewKey, Arc<RealtimeStreamView>>,
    synced_streams: Mutex<Vec<SyncedStreamState>>,
    ntp: Arc<NtpView>,
}

impl PartyViewState {
    pub fn new() -> Self {
        Self {
            realtime_streams: DashMap::new(),
            synced_streams: Mutex::new(Vec::new()),
            ntp: Arc::new(NtpView::new()),
        }
    }

    pub fn realtime_stream(
        &self,
        key: StreamViewKey,
        display_name: String,
    ) -> Arc<RealtimeStreamView> {
        self.realtime_streams
            .entry(key.clone())
            .or_insert_with(|| Arc::new(RealtimeStreamView::new(&key, display_name)))
            .clone()
    }

    pub fn retain_realtime_streams(&self, active: &HashSet<StreamViewKey>) {
        self.realtime_streams
            .retain(|key, _| active.contains(key));
    }

    pub fn realtime_hosts(&self) -> Vec<HostInfo> {
        let mut hosts: Vec<HostInfo> = Vec::new();

        for entry in self.realtime_streams.iter() {
            let key = entry.key().clone();
            let stream = entry.value().stream_info(key);

            if let Some(host) = hosts.iter_mut().find(|h| h.id == stream.key.host_id) {
                host.streams.push(stream);
            } else {
                hosts.push(HostInfo {
                    id: stream.key.host_id,
                    streams: vec![stream],
                });
            }
        }

        hosts.sort_by_key(|h| h.id.to_string());
        for host in &mut hosts {
            host.streams.sort_by(|a, b| {
                a.display_name
                    .cmp(&b.display_name)
                    .then_with(|| a.key.source_addr.cmp(&b.key.source_addr))
            });
        }

        hosts
    }

    pub fn realtime_graph(&self, key: &StreamViewKey) -> Vec<StreamSnapshot> {
        self.realtime_streams
            .get(key)
            .map(|stream| stream.graph())
            .unwrap_or_default()
    }

    pub fn set_synced_streams(&self, streams: Vec<SyncedStreamState>) {
        if let Ok(mut synced_streams) = self.synced_streams.lock() {
            *synced_streams = streams;
        }
    }

    pub fn synced_streams(&self) -> Vec<SyncedStreamState> {
        self.synced_streams
            .lock()
            .map(|streams| streams.clone())
            .unwrap_or_default()
    }

    pub fn update_ntp(&self, info: NtpDebugInfo) {
        self.ntp.update(info);
    }

    pub fn ntp_debug(&self) -> Option<NtpDebugInfo> {
        self.ntp.snapshot()
    }

    pub fn clear(&self) {
        self.realtime_streams.clear();
        if let Ok(mut synced_streams) = self.synced_streams.lock() {
            synced_streams.clear();
        }
    }
}

impl Default for PartyViewState {
    fn default() -> Self {
        Self::new()
    }
}
