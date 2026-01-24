//! Synchronized audio stream (with buffering) for music playback.
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
    pub frame_size: u32,
    pub opus_data: Vec<u8>,
}

impl SyncedFrame {
    pub fn new(stream_id: SyncedStreamId, sequence_number: u64, opus_packet: OpusPacket) -> Self {
        Self {
            stream_id,
            sequence_number,
            frame_size: opus_packet.frame_size as u32,
            opus_data: opus_packet.data,
        }
    }

    pub fn to_opus_packet(&self) -> OpusPacket {
        OpusPacket {
            data: self.opus_data.clone(),
            frame_size: self.frame_size as usize,
        }
    }
}

/// Playback progress for a synced stream.
#[derive(Debug, Clone)]
pub struct SyncedStreamProgress {
    pub frames_played: u64,
    pub buffered_frames: u64,
    pub buffer_ahead_ms: u64,
    pub is_playing: bool,
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

/// A buffer belonging to a (host, stream_id)
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    decoder: crate::audio::opus::OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>,
    opus_frames: HashMap<u64, OpusPacket>,
    last_seen: Instant,
    meta: Option<SyncedStreamMeta>,
    frames_played: u64,

    // Playback state
    playing: bool,
    start_party_time: u64,
    start_seq: u64,

    // JIT decoding cache for the current frame
    current_decode: Option<(u64, AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>)>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    BufferEntry<Sample, CHANNELS, SAMPLE_RATE>
{
    fn new(decoder: crate::audio::opus::OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>) -> Self {
        Self {
            decoder,
            opus_frames: HashMap::new(),
            last_seen: Instant::now(),
            meta: None,
            frames_played: 0,
            playing: false,
            start_party_time: 0,
            start_seq: 1,
            current_decode: None,
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
        // If any existing buffer has a different stream_id, clear all buffers
        let mut should_clear = false;
        for entry in self.buffers.iter() {
            if entry.key().stream_id != meta.stream_id {
                should_clear = true;
                break;
            }
        }

        if should_clear {
            info!(
                "New stream ID {} detected, clearing all old buffers",
                meta.stream_id
            );
            self.buffers.clear();
        }

        let key = BufferKey {
            source_addr,
            stream_id: meta.stream_id,
        };

        let mut entry = self.get_or_create_entry(key);
        entry.last_seen = Instant::now();
        entry.meta = Some(meta);
    }

    pub fn receive_control(
        &self,
        source_addr: SocketAddr,
        control: crate::party::stream::SyncedControl,
    ) {
        let stream_id = match &control {
            crate::party::stream::SyncedControl::Start { stream_id, .. } => *stream_id,
            crate::party::stream::SyncedControl::Pause { stream_id } => *stream_id,
        };

        // If any existing buffer has a different stream_id, clear all buffers
        let mut should_clear = false;
        for entry in self.buffers.iter() {
            if entry.key().stream_id != stream_id {
                should_clear = true;
                break;
            }
        }

        if should_clear {
            info!(
                "Control for new stream ID {} detected, clearing all old buffers",
                stream_id
            );
            self.buffers.clear();
        }

        match control {
            crate::party::stream::SyncedControl::Start {
                stream_id,
                party_clock_time,
                seq,
            } => {
                let key = BufferKey {
                    source_addr,
                    stream_id,
                };
                let mut entry = self.get_or_create_entry(key);
                entry.playing = true;
                entry.start_party_time = party_clock_time;
                entry.start_seq = seq;
                entry.last_seen = Instant::now();
                info!(
                    "Stream {} starting at seq {} at party time {}",
                    stream_id, seq, party_clock_time
                );
            }
            crate::party::stream::SyncedControl::Pause { stream_id } => {
                let key = BufferKey {
                    source_addr,
                    stream_id,
                };
                if let Some(mut entry) = self.buffers.get_mut(&key) {
                    entry.playing = false;
                    entry.last_seen = Instant::now();
                    info!("Stream {} paused", stream_id);
                }
            }
        }
    }

    pub fn receive(&self, source_addr: SocketAddr, frame: SyncedFrame) {
        // If any existing buffer has a different stream_id, clear all buffers
        let mut should_clear = false;
        for entry in self.buffers.iter() {
            if entry.key().stream_id != frame.stream_id {
                should_clear = true;
                break;
            }
        }

        if should_clear {
            info!(
                "Frame for new stream ID {} detected, clearing all old buffers",
                frame.stream_id
            );
            self.buffers.clear();
        }

        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        let mut entry = self.get_or_create_entry(key);
        entry.last_seen = Instant::now();

        entry
            .opus_frames
            .insert(frame.sequence_number, frame.to_opus_packet());
    }

    /// Pulls samples from all streams and mixes them together.
    ///
    /// For each stream, advances through frames whose `play_at` time has been
    /// reached according to the party clock. Frames that are entirely in the
    /// past are skipped (dropped). Partially elapsed frames are sampled from
    /// the correct offset.
    pub fn pull_and_mix(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let party_now = (self.party_now_fn)();
        let us_per_frame = 20_000u64; // 20ms frames
        let us_per_sample = 1_000_000u64 / SAMPLE_RATE as u64;
        let mut mixed: Vec<i64> = vec![0; len];
        let mut source_count = 0usize;
        let mut samples_collected = 0;

        for mut entry in self.buffers.iter_mut() {
            if !entry.playing || entry.start_party_time > party_now {
                continue;
            }

            let mut local_samples: Vec<i64> = vec![0; len];
            let mut local_collected = 0;

            while local_collected < len {
                let current_party_time = party_now + (local_collected as u64 * us_per_sample);
                let elapsed_us = current_party_time - entry.start_party_time;
                let seq_offset = elapsed_us / us_per_frame;
                let seq = entry.start_seq + seq_offset;
                let sample_offset_in_frame = ((elapsed_us % us_per_frame) / us_per_sample) as usize;

                let frame_pcm = if let Some((cached_seq, ref pcm)) = entry.current_decode {
                    if cached_seq == seq { Some(pcm) } else { None }
                } else {
                    None
                };

                let frame_pcm = if let Some(pcm) = frame_pcm {
                    pcm
                } else {
                    if let Some(opus_packet) = entry.opus_frames.get(&seq) {
                        if let Some(pcm) = entry.decoder.decode_packet(opus_packet) {
                            entry.current_decode = Some((seq, pcm));
                            &entry.current_decode.as_ref().unwrap().1
                        } else {
                            break;
                        }
                    } else {
                        // Missing frame - send request for missing frames
                        // (In a real implementation, we'd batch these)
                        break;
                    }
                };

                let frame_data = frame_pcm.data();
                let remaining_in_frame = frame_data.len() - sample_offset_in_frame;
                let take_count = (len - local_collected).min(remaining_in_frame);

                for i in 0..take_count {
                    local_samples[local_collected + i] =
                        frame_data[sample_offset_in_frame + i].to_i64_for_mix();
                }
                local_collected += take_count;
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
            let timed_out = now.duration_since(entry.last_seen) >= SYNCED_STREAM_TIMEOUT;
            if timed_out {
                info!(
                    "Removing synced buffer for {} stream {} (timeout)",
                    key.source_addr, key.stream_id
                );
            }
            !timed_out
        });
    }

    pub fn active_streams(&self) -> Vec<SyncedStreamState> {
        let party_now = (self.party_now_fn)();
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            let buffered_frames = entry.opus_frames.len();

            let frames_played = if entry.playing && party_now > entry.start_party_time {
                (party_now - entry.start_party_time) / 20_000
            } else {
                0
            };

            result.push(SyncedStreamState {
                stream_id: entry.key().stream_id,
                source_addr: entry.key().source_addr,
                meta: entry.meta.clone(),
                progress: SyncedStreamProgress {
                    frames_played,
                    buffered_frames: buffered_frames as u64,
                    buffer_ahead_ms: 0, // Not easily calculated in this model
                    is_playing: entry.playing,
                },
            });
        }

        result
    }

    /// Identifies gaps in the buffered Opus packets and returns them for retransmission requests.
    ///
    /// Checks for missing sequence numbers between the start of the song and a lookahead
    /// window from the current playback position.
    pub fn get_missing_frames(&self) -> Vec<(SocketAddr, SyncedStreamId, Vec<u64>)> {
        let mut result = Vec::new();
        let party_now = (self.party_now_fn)();

        for entry in self.buffers.iter() {
            let Some(meta) = &entry.meta else { continue };

            let mut missing = Vec::new();

            let current_seq = if entry.playing && party_now > entry.start_party_time {
                entry.start_seq + (party_now - entry.start_party_time) / 20_000
            } else {
                entry.start_seq
            };

            // Check up to 200 frames ahead of current position, and all frames before it
            let end_check = (current_seq + 200).min(meta.total_frames);

            // We check from seq 1 because we want the whole song for seeking back
            for seq in 1..=end_check {
                if !entry.opus_frames.contains_key(&seq) {
                    missing.push(seq);
                    // Cap per request to avoid huge packets
                    if missing.len() >= 100 {
                        break;
                    }
                }
            }

            if !missing.is_empty() {
                result.push((entry.key().source_addr, entry.key().stream_id, missing));
            }
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
