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
use symphonia::core::codecs::DecoderOptions;
use tracing::{error, info, warn};

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::audio::symphonia_compat::{WireCodecParams, extract_and_resample};
use crate::pipeline::Source;
use rubato::FftFixedIn;

const SYNCED_STREAM_TIMEOUT: Duration = Duration::from_secs(30);

static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);

pub type SyncedStreamId = u64;

pub fn new_stream_id() -> SyncedStreamId {
    NEXT_STREAM_ID.fetch_add(1, Ordering::Relaxed)
}

/// Metadata about a synced stream, sent over the network.
///
/// Contains information the receiver needs to display and track the stream:
/// - file name for UI display
/// - total frames for progress/completion tracking
/// - codec params for creating the decoder on receiver side
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedStreamMeta {
    pub stream_id: SyncedStreamId,
    pub file_name: String,
    pub total_frames: u64,
    pub total_samples: u64,
    pub codec_params: WireCodecParams,
}

/// A single compressed audio packet for synced playback, sent over the network.
///
/// Contains raw compressed bytes from the original audio file.
/// Timing is calculated from sequence_number + control messages.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedFrame {
    pub stream_id: SyncedStreamId,
    pub sequence_number: u64,
    pub dur: u32,
    pub data: Vec<u8>,
}

/// Raw packet stored for sender-side retransmission.
#[derive(Clone)]
pub struct RawPacket {
    pub dur: u32,
    pub data: Vec<u8>,
}

/// Decoded PCM frame stored in buffer, ready for playback.
#[derive(Clone)]
pub struct DecodedFrame<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pub samples: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
}

impl SyncedFrame {
    pub fn new(stream_id: SyncedStreamId, sequence_number: u64, dur: u32, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            sequence_number,
            dur,
            data,
        }
    }
}

/// Playback progress for a synced stream (output type for GUI).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncedStreamProgress {
    pub samples_played: u64,
    pub total_samples: u64,
    pub buffered_frames: u64,
    pub is_playing: bool,
    pub highest_seq_received: u64,
}

/// Complete state of a synced stream (output type for GUI).
#[derive(Debug, Clone)]
pub struct SyncedStreamState {
    pub stream_id: SyncedStreamId,
    pub meta: SyncedStreamMeta,
    pub progress: SyncedStreamProgress,
    pub is_local_sender: bool,
}

impl PartialEq for SyncedStreamState {
    fn eq(&self, other: &Self) -> bool {
        self.stream_id == other.stream_id
            && self.meta.stream_id == other.meta.stream_id
            && self.meta.file_name == other.meta.file_name
            && self.meta.total_frames == other.meta.total_frames
            && self.progress == other.progress
            && self.is_local_sender == other.is_local_sender
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BufferKey {
    source_addr: SocketAddr,
    stream_id: SyncedStreamId,
}

/// A buffer for a single stream from a single source.
///
/// Created only when metadata arrives with valid decoder.
/// Stores decoded PCM frames for immediate playback.
///
/// Frames must be decoded in sequence order because codecs like MP3/AAC/FLAC
/// are stateful. Out-of-order frames are buffered in `pending_raw` until
/// their predecessors arrive.
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    resampler: Option<FftFixedIn<f32>>,
    meta: SyncedStreamMeta,
    pending_raw: HashMap<u64, SyncedFrame>,
    next_decode_seq: u64,
    decoded_frames: HashMap<u64, DecodedFrame<Sample, CHANNELS, SAMPLE_RATE>>,
    last_seen: Instant,

    playing: bool,
    start_party_time: u64,
    start_seq: u64,
}

fn create_decoder(
    codec_params: &WireCodecParams,
) -> Option<Box<dyn symphonia::core::codecs::Decoder>> {
    let params = codec_params.to_symphonia();
    match symphonia::default::get_codecs().make(&params, &DecoderOptions::default()) {
        Ok(decoder) => {
            info!(
                "Created decoder for codec {:?}, sample_rate={}",
                codec_params.codec, codec_params.sample_rate
            );
            Some(decoder)
        }
        Err(e) => {
            error!("Failed to create decoder: {}", e);
            None
        }
    }
}

/// Manages synchronized audio streams from multiple sources.
pub struct SyncedAudioStreamManager<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffers: DashMap<BufferKey, BufferEntry<Sample, CHANNELS, SAMPLE_RATE>>,
    party_now_fn: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>
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

    /// Receives stream metadata. This is the ONLY place entries are created/deleted.
    ///
    /// - If stream_id differs from existing entries, clears all old entries
    /// - Creates decoder from codec_params; if fails, entry is not created
    /// - Updates existing entry's meta if stream_id matches
    pub fn receive_meta(&self, source_addr: SocketAddr, meta: SyncedStreamMeta) {
        let dominated_by_other_stream = self
            .buffers
            .iter()
            .any(|e| e.key().stream_id != meta.stream_id);

        if dominated_by_other_stream {
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

        // Metadata update
        if let Some(mut entry) = self.buffers.get_mut(&key) {
            entry.meta = meta;
            entry.last_seen = Instant::now();
            return;
        }

        // New metadata, create decoder
        let Some(decoder) = create_decoder(&meta.codec_params) else {
            return;
        };

        let resampler = if meta.codec_params.sample_rate != SAMPLE_RATE {
            match FftFixedIn::<f32>::new(
                meta.codec_params.sample_rate as usize,
                SAMPLE_RATE as usize,
                1024,
                1,
                CHANNELS,
            ) {
                Ok(r) => {
                    info!(
                        "Created resampler {}Hz -> {}Hz for stream {}",
                        meta.codec_params.sample_rate, SAMPLE_RATE, meta.stream_id
                    );
                    Some(r)
                }
                Err(e) => {
                    error!("Failed to create resampler: {}", e);
                    None
                }
            }
        } else {
            None
        };

        info!(
            "Creating synced buffer for source {} stream {}",
            source_addr, meta.stream_id
        );

        self.buffers.insert(
            key,
            BufferEntry {
                decoder,
                resampler,
                meta,
                pending_raw: HashMap::new(),
                next_decode_seq: 1,
                decoded_frames: HashMap::new(),
                last_seen: Instant::now(),
                playing: false,
                start_party_time: 0,
                start_seq: 1,
            },
        );
    }

    /// Receives playback control.
    pub fn receive_control(
        &self,
        source_addr: SocketAddr,
        control: crate::party::stream::SyncedControl,
    ) {
        let stream_id = match &control {
            crate::party::stream::SyncedControl::Start { stream_id, .. } => *stream_id,
            crate::party::stream::SyncedControl::Pause { stream_id } => *stream_id,
        };

        let key = BufferKey {
            source_addr,
            stream_id,
        };

        let Some(mut entry) = self.buffers.get_mut(&key) else {
            warn!("receive_control: StreamID {:?} not found", key);
            return;
        };

        match control {
            crate::party::stream::SyncedControl::Start {
                party_clock_time,
                seq,
                ..
            } => {
                entry.playing = true;
                entry.start_party_time = party_clock_time;
                entry.start_seq = seq;
                entry.last_seen = Instant::now();
                if seq > entry.next_decode_seq {
                    // Seeking forward: update decode position, clear pending
                    entry.next_decode_seq = seq;
                    entry.pending_raw.clear();
                }
                info!(
                    "Stream {:?} starting at seq {} at party time {}",
                    key, seq, party_clock_time
                );
            }
            crate::party::stream::SyncedControl::Pause { .. } => {
                entry.playing = false;
                entry.last_seen = Instant::now();
                info!("Stream {:?} paused", key);
            }
        }
    }

    /// Receives audio frame. Only stores if entry exists.
    ///
    /// Frames are decoded in sequence order to maintain correct decoder state.
    /// Out-of-order frames are buffered until their predecessors arrive.
    pub fn receive(&self, source_addr: SocketAddr, frame: SyncedFrame) {
        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        let Some(mut entry) = self.buffers.get_mut(&key) else {
            return;
        };
        let entry = &mut *entry;

        entry.last_seen = Instant::now();

        let seq = frame.sequence_number;

        // Duplicate or old frame
        if seq < entry.next_decode_seq
            || entry.decoded_frames.contains_key(&seq)
            || entry.pending_raw.contains_key(&seq)
        {
            return;
        }

        if seq == entry.next_decode_seq {
            Self::decode_frame(entry, &frame);
            entry.next_decode_seq += 1;

            while let Some(pending) = entry.pending_raw.remove(&entry.next_decode_seq) {
                Self::decode_frame(entry, &pending);
                entry.next_decode_seq += 1;
            }
        } else {
            entry.pending_raw.insert(seq, frame);
        }
    }

    fn decode_frame<const C: usize, const R: u32>(
        entry: &mut BufferEntry<Sample, C, R>,
        frame: &SyncedFrame,
    ) where
        Sample: AudioSample,
    {
        let packet =
            symphonia::core::formats::Packet::new_from_slice(0, 0, frame.dur as u64, &frame.data);

        let Ok(decoded) = entry.decoder.decode(&packet) else {
            error!("Failed to decode frame seq={}", frame.sequence_number);
            return;
        };

        let buf = extract_and_resample::<Sample, C, R>(&decoded, entry.resampler.as_mut());

        entry
            .decoded_frames
            .insert(frame.sequence_number, DecodedFrame { samples: buf });
    }

    /// Pulls samples from all streams and mixes them together.
    ///
    /// All decoded frames are already resampled to SAMPLE_RATE, so we can directly
    /// calculate elapsed samples from elapsed time without rate conversion.
    pub fn pull_and_mix(
        &self,
        num_frames: usize,
    ) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let party_now = (self.party_now_fn)();
        let mut mixed: AudioBuffer<i64, CHANNELS, SAMPLE_RATE> =
            AudioBuffer::new_zeroed(num_frames);
        let mut source_count = 0usize;
        let mut frames_collected = 0usize;

        // iter each synced stream, although there should only be one
        for entry in self.buffers.iter() {
            if !entry.playing || entry.start_party_time > party_now {
                continue;
            }

            let start_seq = entry.start_seq;
            let start_party_time = entry.start_party_time;

            let mut local_buf: AudioBuffer<i64, CHANNELS, SAMPLE_RATE> =
                AudioBuffer::new_zeroed(num_frames);
            let mut local_frames = 0usize;

            'fill_local: while local_frames < num_frames {
                let elapsed_samples = (party_now - start_party_time) * SAMPLE_RATE as u64
                    / 1_000_000
                    + local_frames as u64;

                // Find which frame contains the `elapsed_samples`th sample
                let (frame, frame_start_sample) = {
                    let mut cumulative = 0u64;
                    let mut seq = start_seq;
                    loop {
                        let Some(frame) = entry.decoded_frames.get(&seq) else {
                            // Have loss packet, can't calculate where to read. Give up.
                            break 'fill_local;
                        };
                        let frame_end = cumulative + frame.samples.samples_per_channel() as u64;
                        if elapsed_samples < frame_end {
                            break (frame, cumulative);
                        }
                        cumulative = frame_end;
                        seq += 1;
                    }
                };

                let frame_offset = (elapsed_samples - frame_start_sample) as usize;
                let remaining = frame
                    .samples
                    .samples_per_channel()
                    .saturating_sub(frame_offset);
                let take_frames = (num_frames - local_frames).min(remaining);

                for f in 0..take_frames {
                    for ch in 0..CHANNELS {
                        *local_buf.get_mut(local_frames + f, ch) =
                            frame.samples.get(frame_offset + f, ch).to_i64_for_mix();
                    }
                }
                local_frames += take_frames;
            }

            // Add local buffer to mix buffer
            if local_frames > 0 {
                source_count += 1;
                for f in 0..local_frames {
                    for ch in 0..CHANNELS {
                        *mixed.get_mut(f, ch) += *local_buf.get(f, ch);
                    }
                }
                frames_collected = frames_collected.max(local_frames);
            }
        }

        if source_count == 0 {
            return None;
        }

        let divided_samples: Vec<Sample> = mixed
            .into_inner()
            .into_iter()
            .map(|s| Sample::from_i64_mixed(s, source_count))
            .collect();
        AudioBuffer::new(divided_samples).ok()
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
            let buffered_frames = entry.decoded_frames.len();
            let highest_seq_received = entry.decoded_frames.keys().max().copied().unwrap_or(0);

            let mut total_samples = 0u64;
            for frame in entry.decoded_frames.values() {
                total_samples += frame.samples.samples_per_channel() as u64;
            }

            let samples_played = if entry.playing && party_now > entry.start_party_time {
                let mut cumulative_samples = 0u64;
                for seq in entry.start_seq.. {
                    if let Some(frame) = entry.decoded_frames.get(&seq) {
                        cumulative_samples += frame.samples.samples_per_channel() as u64;
                    } else {
                        break;
                    }
                }
                let elapsed_us = party_now - entry.start_party_time;
                let elapsed_samples = elapsed_us * SAMPLE_RATE as u64 / 1_000_000;
                elapsed_samples.min(cumulative_samples)
            } else {
                0
            };

            let is_local_sender =
                entry.key().source_addr.ip().is_loopback() && entry.key().source_addr.port() == 0;

            result.push(SyncedStreamState {
                stream_id: entry.key().stream_id,
                meta: entry.meta.clone(),
                progress: SyncedStreamProgress {
                    samples_played,
                    total_samples,
                    buffered_frames: buffered_frames as u64,
                    is_playing: entry.playing,
                    highest_seq_received,
                },
                is_local_sender,
            });
        }

        result
    }

    /// Identifies gaps in the buffered packets and returns them for retransmission requests.
    pub fn get_missing_frames(&self) -> Vec<(SocketAddr, SyncedStreamId, Vec<u64>)> {
        let mut result = Vec::new();
        let party_now = (self.party_now_fn)();

        for entry in self.buffers.iter() {
            let mut missing = Vec::new();

            let current_seq = if entry.playing && party_now > entry.start_party_time {
                let elapsed_us = party_now - entry.start_party_time;
                let elapsed_samples = elapsed_us * SAMPLE_RATE as u64 / 1_000_000;
                let mut cumulative = 0u64;
                let mut seq = entry.start_seq;
                while let Some(frame) = entry.decoded_frames.get(&seq) {
                    if cumulative >= elapsed_samples {
                        break;
                    }
                    cumulative += frame.samples.samples_per_channel() as u64;
                    seq += 1;
                }
                seq
            } else {
                entry.start_seq
            };

            let end_check = (current_seq + 200).min(entry.meta.total_frames);

            for seq in 1..=end_check {
                if !entry.decoded_frames.contains_key(&seq) {
                    missing.push(seq);
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
    for SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        let num_frames = len / CHANNELS;
        self.pull_and_mix(num_frames)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        let num_frames = len / CHANNELS;
        self.pull_and_mix(num_frames)
    }
}
