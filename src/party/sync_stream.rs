//! Synchronized audio stream for music playback.
//!
//! Unlike realtime streams that play immediately, synced streams buffer audio
//! and play at a specified party clock time, ensuring all participants hear
//! the same audio at the same moment.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::audio::opus::OpusPacket;
use crate::pipeline::Source;

const SYNCED_BUFFER_CAPACITY: usize = 512;
const SYNCED_STREAM_TIMEOUT: Duration = Duration::from_secs(30);

static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);

pub type SyncedStreamId = u64;

pub fn new_stream_id() -> SyncedStreamId {
    NEXT_STREAM_ID.fetch_add(1, Ordering::Relaxed)
}

/// Metadata about a synced stream, sent over the network.
///
/// Contains information the receiver needs to display and track the stream:
/// file name for UI display, total frames for progress/completion tracking.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedStreamMeta {
    pub stream_id: SyncedStreamId,
    pub file_name: String,
    pub total_frames: u64,
}

/// A single audio frame for synced playback, sent over the network.
///
/// Each frame carries Opus-encoded audio data along with timing information
/// (`play_at`) that tells receivers exactly when to play it according to
/// the party clock.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedFrame {
    pub stream_id: SyncedStreamId,
    pub sequence_number: u64,
    pub play_at: u64,
    pub opus_data: Vec<u8>,
}

impl SyncedFrame {
    pub fn new(
        stream_id: SyncedStreamId,
        sequence_number: u64,
        play_at: u64,
        opus_packet: OpusPacket,
    ) -> Self {
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

/// Playback progress for a synced stream.
#[derive(Debug, Clone)]
pub struct SyncedStreamProgress {
    pub frames_played: u64,
    pub buffered_frames: u64,
    pub buffer_ahead_ms: u64,
}

/// Complete state of a synced stream, used only by `active_streams()`.
///
/// Combines wire metadata (if received) with local playback progress.
#[derive(Debug, Clone)]
pub struct SyncedStreamState {
    pub stream_id: SyncedStreamId,
    pub source_addr: SocketAddr,
    pub meta: Option<SyncedStreamMeta>,
    pub progress: SyncedStreamProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BufferKey {
    source_addr: SocketAddr,
    stream_id: SyncedStreamId,
}

struct DecodedFrame<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    play_at: u64,
    duration_us: u64,
    samples: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
}

/// A buffer belonging to a (host, stream_id)
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    decoder: crate::audio::opus::OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>,
    frames: HashMap<u64, DecodedFrame<Sample, CHANNELS, SAMPLE_RATE>>,
    read_seq: u64,
    last_seen: Instant,
    meta: Option<SyncedStreamMeta>,
    frames_played: u64,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    BufferEntry<Sample, CHANNELS, SAMPLE_RATE>
{
    fn new(decoder: crate::audio::opus::OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>) -> Self {
        Self {
            decoder,
            frames: HashMap::new(),
            read_seq: 1,
            last_seen: Instant::now(),
            meta: None,
            frames_played: 0,
        }
    }
}

/// Manages synchronized audio streams from multiple sources.
///
/// Receives Opus-encoded frames with `play_at` timestamps, decodes them,
/// and mixes audio from all sources when the party clock reaches the
/// scheduled play time.
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

    fn get_or_create_entry(
        &self,
        key: BufferKey,
    ) -> dashmap::mapref::one::RefMut<'_, BufferKey, BufferEntry<Sample, CHANNELS, SAMPLE_RATE>>
    {
        self.buffers.entry(key).or_insert_with(|| {
            info!(
                "Creating synced buffer for source {} stream {}",
                key.source_addr, key.stream_id
            );
            let decoder =
                crate::audio::opus::OpusDecoder::new().expect("Failed to create Opus decoder");
            BufferEntry::new(decoder)
        })
    }

    pub fn receive_meta(&self, source_addr: SocketAddr, meta: SyncedStreamMeta) {
        let key = BufferKey {
            source_addr,
            stream_id: meta.stream_id,
        };

        let mut entry = self.get_or_create_entry(key);
        entry.last_seen = Instant::now();
        entry.meta = Some(meta);
    }

    pub fn receive(&self, source_addr: SocketAddr, frame: SyncedFrame) {
        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        let mut entry = self.get_or_create_entry(key);
        entry.last_seen = Instant::now();

        if frame.sequence_number < entry.read_seq {
            return;
        }

        let opus_packet = frame.to_opus_packet();
        if let Some(pcm_buffer) = entry.decoder.decode_packet(&opus_packet) {
            if entry.frames.len() < SYNCED_BUFFER_CAPACITY {
                let samples_per_channel = pcm_buffer.samples_per_channel();
                let duration_us = (samples_per_channel as u64 * 1_000_000) / SAMPLE_RATE as u64;
                entry.frames.insert(
                    frame.sequence_number,
                    DecodedFrame {
                        play_at: frame.play_at,
                        duration_us,
                        samples: pcm_buffer,
                    },
                );
            } else {
                warn!(
                    "Synced buffer full for stream {}, dropping frame",
                    frame.stream_id
                );
            }
        }
    }

    /// Pulls samples from all streams and mixes them together.
    ///
    /// For each stream, advances through frames whose `play_at` time has been
    /// reached according to the party clock. Frames that are entirely in the
    /// past are skipped (dropped). Partially elapsed frames are sampled from
    /// the correct offset.
    pub fn pull_and_mix(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let party_now = (self.party_now_fn)();
        let us_per_sample = 1_000_000u64 / SAMPLE_RATE as u64;
        let mut mixed: Vec<i64> = vec![0; len];
        let mut source_count = 0usize;
        let mut samples_collected = 0;

        for mut entry in self.buffers.iter_mut() {
            let mut local_samples: Vec<i64> = vec![0; len];
            let mut local_collected = 0;

            loop {
                let seq = entry.read_seq;
                let Some(frame) = entry.frames.get(&seq) else {
                    break;
                };

                let frame_end = frame.play_at + frame.duration_us;

                // Frame is entirely in the past - skip it
                if frame_end <= party_now {
                    entry.frames.remove(&seq);
                    entry.read_seq += 1;
                    entry.frames_played += 1;
                    continue;
                }

                // Frame hasn't started yet - wait
                if frame.play_at > party_now {
                    break;
                }

                // Frame is currently playing: play_at <= party_now < frame_end
                let frame_samples = frame.samples.data();
                let elapsed_us = party_now - frame.play_at;
                let sample_offset =
                    ((elapsed_us / us_per_sample) as usize).min(frame_samples.len());
                let remaining_in_frame = frame_samples.len() - sample_offset;
                let take_count = (len - local_collected).min(remaining_in_frame);

                for (i, sample) in frame_samples[sample_offset..sample_offset + take_count]
                    .iter()
                    .enumerate()
                {
                    local_samples[local_collected + i] = sample.to_i64_for_mix();
                }
                local_collected += take_count;

                // If we consumed the entire remaining frame, remove it
                if sample_offset + take_count >= frame_samples.len() {
                    entry.frames.remove(&seq);
                    entry.read_seq += 1;
                    entry.frames_played += 1;
                }

                if local_collected >= len {
                    break;
                }
            }

            if local_collected > 0 {
                source_count += 1;
                for i in 0..local_collected {
                    mixed[i] += local_samples[i];
                }
                samples_collected = samples_collected.max(local_collected);
            }
        }

        if source_count == 0 {
            return None;
        }

        let samples: Vec<Sample> = mixed
            .into_iter()
            .map(|s| Sample::from_i64_mixed(s, source_count))
            .collect();
        AudioBuffer::new(samples).ok()
    }

    pub fn cleanup_stale(&self) {
        let now = Instant::now();
        self.buffers.retain(|key, entry| {
            let is_complete = entry
                .meta
                .as_ref()
                .map(|m| entry.frames_played >= m.total_frames && entry.frames.is_empty())
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

    pub fn active_streams(&self) -> Vec<SyncedStreamState> {
        let party_now = (self.party_now_fn)();
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            let buffered_frames = entry.frames.len();
            let next_frame = entry.frames.get(&entry.read_seq);
            let buffer_ahead_ms = next_frame
                .map(|f| {
                    if f.play_at > party_now {
                        (f.play_at - party_now) / 1000
                    } else {
                        0
                    }
                })
                .unwrap_or(0);

            result.push(SyncedStreamState {
                stream_id: entry.key().stream_id,
                source_addr: entry.key().source_addr,
                meta: entry.meta.clone(),
                progress: SyncedStreamProgress {
                    frames_played: entry.frames_played,
                    buffered_frames: buffered_frames as u64,
                    buffer_ahead_ms,
                },
            });
        }

        result
    }
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
