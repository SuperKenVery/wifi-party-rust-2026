//! Audio stream abstraction for network transport.
//!
//! This module defines the core abstraction for audio streams that can be sent
//! over the network. Each stream type (e.g., realtime, synced) has its own
//! frame format and processing logic.
//!
//! # Architecture
//!
//! ```text
//! NetworkPacket (enum)
//!     ├── Realtime(RealtimeFrame)  ──► RealtimeAudioStream
//!     │       └── stream_id: Mic/System/...
//!     │       └── Each (HostId, StreamId) gets its own JitterBuffer
//!     │
//!     └── Future: Synced(SyncedFrame) ──► SyncedAudioStream
//! ```
//!
//! # Key Types
//!
//! - [`NetworkPacket`] - Top-level enum sent over the wire
//! - [`RealtimeStreamId`] - Identifies realtime stream instances (Mic, System, etc.)
//! - [`RealtimeFrame`] - Frame format for realtime audio
//! - [`RealtimeAudioStream`] - Manages all realtime streams across all hosts

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use tracing::info;

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::JitterBuffer;
use crate::pipeline::{Sink, Source};
use crate::state::HostId;

const HOST_TIMEOUT: Duration = Duration::from_secs(5);
const JITTER_BUFFER_CAPACITY: usize = 64;

/// Identifies a realtime audio stream instance.
///
/// Each variant represents a different audio source that uses realtime
/// streaming with jitter buffering.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[rkyv(compare(PartialEq))]
pub enum RealtimeStreamId {
    Mic,
    System,
}

impl std::fmt::Display for RealtimeStreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RealtimeStreamId::Mic => write!(f, "Mic"),
            RealtimeStreamId::System => write!(f, "System"),
        }
    }
}

/// Frame format for realtime audio streams.
///
/// Contains the stream identifier, sequence number for ordering,
/// timestamp for synchronization, and the audio samples.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct RealtimeFrame<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pub stream_id: RealtimeStreamId,
    pub sequence_number: u64,
    pub timestamp: u64,
    pub samples: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    RealtimeFrame<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        stream_id: RealtimeStreamId,
        sequence_number: u64,
        samples: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Self {
            stream_id,
            sequence_number,
            timestamp,
            samples,
        }
    }

    pub fn samples_per_channel(&self) -> usize {
        self.samples.samples_per_channel()
    }
}

/// Top-level network packet enum.
///
/// All data sent over the network is wrapped in this enum, allowing
/// the receiver to dispatch to the appropriate stream handler.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum NetworkPacket<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    Realtime(RealtimeFrame<Sample, CHANNELS, SAMPLE_RATE>),
}

/// Internal frame type used by JitterBuffer.
///
/// This is a simple wrapper that the JitterBuffer expects, containing
/// just sequence number, timestamp, and samples.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct JitterBufferFrame<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pub sequence_number: u64,
    pub timestamp: u64,
    pub samples: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    JitterBufferFrame<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn from_realtime(frame: &RealtimeFrame<Sample, CHANNELS, SAMPLE_RATE>) -> Self
    where
        Sample: Clone,
    {
        Self {
            sequence_number: frame.sequence_number,
            timestamp: frame.timestamp,
            samples: frame.samples.clone(),
        }
    }
}

/// Key for identifying a specific jitter buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BufferKey {
    host_id: HostId,
    stream_id: RealtimeStreamId,
}

/// Metadata for a buffer entry.
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffer: JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>,
    last_seen: Instant,
}

/// Manages all realtime audio streams across all hosts.
///
/// Each (HostId, RealtimeStreamId) pair gets its own jitter buffer.
/// When pulling audio, all buffers are mixed together into a single output.
///
/// # Thread Safety
///
/// Uses [`DashMap`] internally for lock-free concurrent access between
/// the network receiver thread (pushes frames) and the audio output thread
/// (pulls mixed frames).
pub struct RealtimeAudioStream<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffers: DashMap<BufferKey, BufferEntry<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            buffers: DashMap::new(),
        }
    }

    /// Receives a realtime frame and routes it to the appropriate jitter buffer.
    ///
    /// Creates a new buffer if this is the first frame from this (host, stream) pair.
    pub fn receive(&self, host_id: HostId, frame: RealtimeFrame<Sample, CHANNELS, SAMPLE_RATE>)
    where
        Sample: Clone,
    {
        let key = BufferKey {
            host_id,
            stream_id: frame.stream_id,
        };

        let mut entry = self.buffers.entry(key).or_insert_with(|| {
            info!(
                "Creating jitter buffer for host {} stream {:?}",
                host_id.to_string(),
                frame.stream_id
            );
            BufferEntry {
                buffer: JitterBuffer::new(JITTER_BUFFER_CAPACITY),
                last_seen: Instant::now(),
            }
        });

        entry.last_seen = Instant::now();

        use crate::audio::frame::AudioFrame;
        let jitter_frame = AudioFrame {
            sequence_number: frame.sequence_number,
            timestamp: frame.timestamp,
            samples: frame.samples,
        };
        entry.buffer.push(jitter_frame);
    }

    /// Pulls `len` samples from all buffers and mixes them together.
    ///
    /// Returns `None` if no buffers have data available.
    pub fn pull_and_mix(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut mixed: Vec<f64> = vec![0.0; len];
        let mut has_data = false;

        for entry in self.buffers.iter_mut() {
            if let Some(frame) = entry.buffer.pull(len) {
                has_data = true;
                for (i, sample) in frame.samples.data().iter().enumerate() {
                    if i < len {
                        mixed[i] += sample.to_f64_normalized();
                    }
                }
            }
        }

        if !has_data {
            return None;
        }

        let samples: Vec<Sample> = mixed.into_iter().map(Sample::from_f64_normalized).collect();
        AudioBuffer::new(samples).ok()
    }

    /// Removes buffers that haven't received data within the timeout period.
    pub fn cleanup_stale(&self) {
        let now = Instant::now();
        self.buffers.retain(|key, entry| {
            let alive = now.duration_since(entry.last_seen) < HOST_TIMEOUT;
            if !alive {
                info!(
                    "Removing stale buffer for host {} stream {:?}",
                    key.host_id.to_string(),
                    key.stream_id
                );
            }
            alive
        });
    }

    /// Returns the number of active buffers.
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Returns a list of unique active host IDs.
    pub fn active_hosts(&self) -> Vec<HostId> {
        let mut hosts = Vec::new();
        for entry in self.buffers.iter() {
            let host_id = entry.key().host_id;
            if !hosts.contains(&host_id) {
                hosts.push(host_id);
            }
        }
        hosts
    }

    /// Returns stats for all streams belonging to a specific host.
    pub fn host_stream_stats(&self, host_id: HostId) -> Vec<StreamStats> {
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            if entry.key().host_id != host_id {
                continue;
            }

            let stream_id = entry.key().stream_id;
            let stats = entry.value().buffer.stats();

            let packet_loss = 1.0 - stats.stability();

            // Hardware latency = frame_size / sample_rate / channels * 1000
            let expected_frame_size = stats.expected_frame_size();
            let hardware_latency_ms = if expected_frame_size > 0 {
                expected_frame_size as f64 / (SAMPLE_RATE as f64) / (CHANNELS as f64) * 1000.0
            } else {
                0.0
            };
            let jitter_latency_ms = stats.latency_ema() * hardware_latency_ms;

            result.push(StreamStats {
                stream_id,
                audio_level: stats.audio_level_ema() as f32,
                packet_loss: packet_loss as f32,
                jitter_latency_ms: jitter_latency_ms as f32,
                hardware_latency_ms: hardware_latency_ms as f32,
            });
        }

        result
    }
}

/// Statistics for a single audio stream.
#[derive(Debug, Clone)]
pub struct StreamStats {
    pub stream_id: RealtimeStreamId,
    pub audio_level: f32,
    pub packet_loss: f32,
    pub jitter_latency_ms: f32,
    pub hardware_latency_ms: f32,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for std::sync::Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}

/// Packs AudioBuffer into NetworkPacket::Realtime with the given stream ID.
///
/// Each instance maintains its own sequence counter for independent
/// packet ordering per stream.
pub struct RealtimeFramePacker<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    stream_id: RealtimeStreamId,
    sequence_number: AtomicU64,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    RealtimeFramePacker<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(stream_id: RealtimeStreamId) -> Self {
        Self {
            stream_id,
            sequence_number: AtomicU64::new(0),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> crate::pipeline::Node
    for RealtimeFramePacker<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let seq = self.sequence_number.fetch_add(1, Ordering::Relaxed) + 1;
        let frame = RealtimeFrame::new(self.stream_id, seq, input);
        Some(NetworkPacket::Realtime(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_realtime_frame_creation() {
        let samples = AudioBuffer::<i16, 2, 48000>::new(vec![100i16, -100, 200, -200]).unwrap();
        let frame = RealtimeFrame::new(RealtimeStreamId::Mic, 1, samples);

        assert_eq!(frame.stream_id, RealtimeStreamId::Mic);
        assert_eq!(frame.sequence_number, 1);
        assert_eq!(frame.samples_per_channel(), 2);
    }

    #[test]
    fn test_network_packet_realtime() {
        let samples = AudioBuffer::<i16, 2, 48000>::new(vec![0i16; 960]).unwrap();
        let frame = RealtimeFrame::new(RealtimeStreamId::System, 42, samples);
        let packet: NetworkPacket<i16, 2, 48000> = NetworkPacket::Realtime(frame);

        match packet {
            NetworkPacket::Realtime(f) => {
                assert_eq!(f.stream_id, RealtimeStreamId::System);
                assert_eq!(f.sequence_number, 42);
            }
        }
    }
}
