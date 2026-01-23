//! Synchronized audio stream for music playback.
//!
//! Unlike realtime streams that play immediately, synced streams buffer audio
//! and play at a specified party clock time, ensuring all participants hear
//! the same audio at the same moment.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use tracing::{info, warn};

use crate::audio::frame::AudioBuffer;
use crate::audio::opus::OpusPacket;
use crate::audio::AudioSample;
use crate::pipeline::Source;

const SYNCED_BUFFER_CAPACITY: usize = 512;
const SYNCED_STREAM_TIMEOUT: Duration = Duration::from_secs(30);

static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);

pub type SyncedStreamId = u64;

pub fn new_stream_id() -> SyncedStreamId {
    NEXT_STREAM_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedStreamMeta {
    pub stream_id: SyncedStreamId,
    pub file_name: String,
    pub total_frames: u64,
}

#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedFrame {
    pub stream_id: SyncedStreamId,
    pub sequence_number: u64,
    pub play_at: u64,
    pub opus_data: Vec<u8>,
}

impl SyncedFrame {
    pub fn new(stream_id: SyncedStreamId, sequence_number: u64, play_at: u64, opus_packet: OpusPacket) -> Self {
        Self {
            stream_id,
            sequence_number,
            play_at,
            opus_data: opus_packet.data,
        }
    }

    pub fn to_opus_packet(&self) -> OpusPacket {
        OpusPacket {
            data: self.opus_data.clone(),
            frame_size: 960 * 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BufferKey {
    source_addr: SocketAddr,
    stream_id: SyncedStreamId,
}

struct DecodedFrame<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    play_at: u64,
    samples: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
}

struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    decoder: crate::audio::opus::OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>,
    frames: BTreeMap<u64, DecodedFrame<Sample, CHANNELS, SAMPLE_RATE>>,
    last_seen: Instant,
    file_name: Option<String>,
    total_frames: Option<u64>,
    frames_played: u64,
}

pub struct SyncedAudioStream<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffers: DashMap<BufferKey, BufferEntry<Sample, CHANNELS, SAMPLE_RATE>>,
    party_now_fn: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new<F>(party_now_fn: F) -> Self
    where
        F: Fn() -> u64 + Send + Sync + 'static,
    {
        Self {
            buffers: DashMap::new(),
            party_now_fn: Arc::new(party_now_fn),
        }
    }

    pub fn receive_meta(&self, source_addr: SocketAddr, meta: SyncedStreamMeta) {
        let key = BufferKey {
            source_addr,
            stream_id: meta.stream_id,
        };

        let mut entry = self.buffers.entry(key).or_insert_with(|| {
            info!(
                "Creating synced buffer for source {} stream {} ({})",
                source_addr, meta.stream_id, meta.file_name
            );
            let decoder = crate::audio::opus::OpusDecoder::new()
                .expect("Failed to create Opus decoder");
            BufferEntry {
                decoder,
                frames: BTreeMap::new(),
                last_seen: Instant::now(),
                file_name: None,
                total_frames: None,
                frames_played: 0,
            }
        });

        entry.last_seen = Instant::now();
        entry.file_name = Some(meta.file_name);
        entry.total_frames = Some(meta.total_frames);
    }

    pub fn receive(&self, source_addr: SocketAddr, frame: SyncedFrame) {
        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        let mut entry = self.buffers.entry(key).or_insert_with(|| {
            info!(
                "Creating synced buffer for source {} stream {}",
                source_addr, frame.stream_id
            );
            let decoder = crate::audio::opus::OpusDecoder::new()
                .expect("Failed to create Opus decoder");
            BufferEntry {
                decoder,
                frames: BTreeMap::new(),
                last_seen: Instant::now(),
                file_name: None,
                total_frames: None,
                frames_played: 0,
            }
        });

        entry.last_seen = Instant::now();

        let opus_packet = frame.to_opus_packet();
        if let Some(pcm_buffer) = entry.decoder.decode_packet(&opus_packet) {
            if entry.frames.len() < SYNCED_BUFFER_CAPACITY {
                entry.frames.insert(
                    frame.sequence_number,
                    DecodedFrame {
                        play_at: frame.play_at,
                        samples: pcm_buffer,
                    },
                );
            } else {
                warn!("Synced buffer full for stream {}, dropping frame", frame.stream_id);
            }
        }
    }

    pub fn pull_and_mix(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let party_now = (self.party_now_fn)();
        let mut mixed: Vec<f64> = vec![0.0; len];
        let mut has_data = false;
        let mut samples_collected = 0;

        for mut entry in self.buffers.iter_mut() {
            let mut frames_to_remove = Vec::new();
            let mut local_samples: Vec<f64> = vec![0.0; len];
            let mut local_collected = 0;

            for (&seq, frame) in entry.frames.iter() {
                if frame.play_at <= party_now {
                    let frame_samples = frame.samples.data();
                    let take_count = (len - local_collected).min(frame_samples.len());

                    for (i, sample) in frame_samples.iter().take(take_count).enumerate() {
                        local_samples[local_collected + i] = sample.to_f64_normalized();
                    }
                    local_collected += take_count;
                    frames_to_remove.push(seq);

                    if local_collected >= len {
                        break;
                    }
                }
            }

            for seq in frames_to_remove {
                entry.frames.remove(&seq);
                entry.frames_played += 1;
            }

            if local_collected > 0 {
                has_data = true;
                for i in 0..local_collected {
                    mixed[i] += local_samples[i];
                }
                samples_collected = samples_collected.max(local_collected);
            }
        }

        if !has_data {
            return None;
        }

        let samples: Vec<Sample> = mixed.into_iter().map(Sample::from_f64_normalized).collect();
        AudioBuffer::new(samples).ok()
    }

    pub fn cleanup_stale(&self) {
        let now = Instant::now();
        self.buffers.retain(|key, entry| {
            let is_complete = entry.total_frames
                .map(|total| entry.frames_played >= total && entry.frames.is_empty())
                .unwrap_or(false);

            let timed_out = now.duration_since(entry.last_seen) >= SYNCED_STREAM_TIMEOUT;

            let keep = !is_complete && !timed_out;
            if !keep {
                info!(
                    "Removing synced buffer for {} stream {} (complete={}, timeout={})",
                    key.source_addr, key.stream_id, is_complete, timed_out
                );
            }
            keep
        });
    }

    pub fn active_streams(&self) -> Vec<SyncedStreamInfo> {
        let party_now = (self.party_now_fn)();
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            let buffered_frames = entry.frames.len();
            let next_play_at = entry.frames.values().next().map(|f| f.play_at);
            let buffer_ahead_ms = next_play_at
                .map(|t| if t > party_now { (t - party_now) / 1000 } else { 0 })
                .unwrap_or(0);

            result.push(SyncedStreamInfo {
                stream_id: entry.key().stream_id,
                source_addr: entry.key().source_addr,
                file_name: entry.file_name.clone().unwrap_or_default(),
                frames_played: entry.frames_played,
                total_frames: entry.total_frames,
                buffered_frames: buffered_frames as u64,
                buffer_ahead_ms,
            });
        }

        result
    }
}

#[derive(Debug, Clone)]
pub struct SyncedStreamInfo {
    pub stream_id: SyncedStreamId,
    pub source_addr: SocketAddr,
    pub file_name: String,
    pub frames_played: u64,
    pub total_frames: Option<u64>,
    pub buffered_frames: u64,
    pub buffer_ahead_ms: u64,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for Arc<SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}
