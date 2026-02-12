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
use tracing::{error, info};

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::audio::symphonia_compat::{WireCodecParams, extract_samples};
use crate::pipeline::Source;

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

/// Raw packet stored in buffer, ready for decoding.
#[derive(Clone)]
pub struct RawPacket {
    pub dur: u32,
    pub data: Vec<u8>,
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

    pub fn to_raw_packet(&self) -> RawPacket {
        RawPacket {
            dur: self.dur,
            data: self.data.clone(),
        }
    }
}

/// Playback progress for a synced stream (output type for GUI).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncedStreamProgress {
    pub frames_played: u64,
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
/// Stores raw compressed packets and decodes on demand during playback.
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    meta: SyncedStreamMeta,
    raw_frames: HashMap<u64, RawPacket>,
    last_seen: Instant,

    playing: bool,
    start_party_time: u64,
    start_seq: u64,

    /// Cached decoded PCM for the current packet.
    /// (packet sequence number, sample offset from stream start, decoded PCM)
    current_decode: Option<(u64, u64, AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>)>,
    _marker: std::marker::PhantomData<Sample>,
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

        if let Some(mut entry) = self.buffers.get_mut(&key) {
            entry.meta = meta;
            entry.last_seen = Instant::now();
            return;
        }

        let Some(decoder) = create_decoder(&meta.codec_params) else {
            return;
        };

        info!(
            "Creating synced buffer for source {} stream {}",
            source_addr, meta.stream_id
        );

        self.buffers.insert(
            key,
            BufferEntry {
                decoder,
                meta,
                raw_frames: HashMap::new(),
                last_seen: Instant::now(),
                playing: false,
                start_party_time: 0,
                start_seq: 1,
                current_decode: None,
                _marker: std::marker::PhantomData,
            },
        );
    }

    /// Receives playback control. Only updates existing entries.
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
                info!(
                    "Stream {} starting at seq {} at party time {}",
                    stream_id, seq, party_clock_time
                );
            }
            crate::party::stream::SyncedControl::Pause { .. } => {
                entry.playing = false;
                entry.last_seen = Instant::now();
                info!("Stream {} paused", stream_id);
            }
        }
    }

    /// Receives audio frame. Only stores if entry exists.
    pub fn receive(&self, source_addr: SocketAddr, frame: SyncedFrame) {
        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        let Some(mut entry) = self.buffers.get_mut(&key) else {
            return;
        };

        entry.last_seen = Instant::now();
        entry
            .raw_frames
            .insert(frame.sequence_number, frame.to_raw_packet());
    }

    /// Pulls samples from all streams and mixes them together.
    pub fn pull_and_mix(
        &self,
        num_frames: usize,
    ) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let party_now = (self.party_now_fn)();
        let mut mixed: AudioBuffer<i64, CHANNELS, SAMPLE_RATE> =
            AudioBuffer::new_zeroed(num_frames);
        let mut source_count = 0usize;
        let mut frames_collected = 0usize;

        for mut entry in self.buffers.iter_mut() {
            if !entry.playing || entry.start_party_time > party_now {
                continue;
            }

            let source_sample_rate = entry.meta.codec_params.sample_rate;
            let start_seq = entry.start_seq;
            let start_party_time = entry.start_party_time;

            let mut local_buf: AudioBuffer<i64, CHANNELS, SAMPLE_RATE> =
                AudioBuffer::new_zeroed(num_frames);
            let mut local_frames = 0usize;

            while local_frames < num_frames {
                let elapsed_us = party_now - start_party_time;
                let elapsed_samples_at_source =
                    (elapsed_us * source_sample_rate as u64 / 1_000_000) + local_frames as u64;

                // Check if elapsed sample falls within the already-decoded PCM buffer
                let cached_match = if let Some((_, cached_offset, ref pcm)) = entry.current_decode {
                    elapsed_samples_at_source >= cached_offset
                        && elapsed_samples_at_source
                            < cached_offset + pcm.samples_per_channel() as u64
                } else {
                    false
                };

                // decode a packet
                if !cached_match {
                    // find the right packet to decode
                    let mut cumulative = 0u64;
                    let mut target_seq = start_seq;
                    loop {
                        if let Some(pkt) = entry.raw_frames.get(&target_seq) {
                            if elapsed_samples_at_source < cumulative + pkt.dur as u64 {
                                break;
                            }
                            cumulative += pkt.dur as u64;
                            target_seq += 1;
                        } else {
                            break;
                        }
                    }

                    let raw_data = entry
                        .raw_frames
                        .get(&target_seq)
                        .map(|pkt| (pkt.dur, pkt.data.clone()));

                    if let Some((dur, data)) = raw_data {
                        let packet = symphonia::core::formats::Packet::new_from_slice(
                            0, cumulative, dur as u64, &data,
                        );

                        if let Ok(decoded) = entry.decoder.decode(&packet) {
                            let channel_bufs: [Vec<Sample>; CHANNELS] =
                                extract_samples::<Sample, CHANNELS>(&decoded);

                            let samples_count = channel_bufs[0].len();
                            let mut samples: Vec<Sample> =
                                Vec::with_capacity(samples_count * CHANNELS);
                            for i in 0..samples_count {
                                for ch in 0..CHANNELS {
                                    samples.push(channel_bufs[ch][i]);
                                }
                            }

                            if let Ok(buf) = AudioBuffer::new(samples) {
                                entry.current_decode = Some((target_seq, cumulative, buf));
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    } else {
                        // missing packet
                        break;
                    }
                }

                let (pcm_offset, pcm) = match &entry.current_decode {
                    Some((_, offset, pcm)) => (*offset, pcm),
                    None => break,
                };

                // Position within the decoded PCM buffer
                let frame_offset = (elapsed_samples_at_source - pcm_offset) as usize;
                let remaining = pcm.samples_per_channel().saturating_sub(frame_offset);
                let take_frames = (num_frames - local_frames).min(remaining);

                for f in 0..take_frames {
                    for ch in 0..CHANNELS {
                        *local_buf.get_mut(local_frames + f, ch) =
                            pcm.get(frame_offset + f, ch).to_i64_for_mix();
                    }
                }
                local_frames += take_frames;
            }

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

        let samples: Vec<Sample> = mixed
            .into_inner()
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
            let buffered_frames = entry.raw_frames.len();
            let highest_seq_received = entry.raw_frames.keys().max().copied().unwrap_or(0);

            let frames_played = if entry.playing && party_now > entry.start_party_time {
                let mut cumulative_samples = 0u64;
                for seq in entry.start_seq.. {
                    if let Some(pkt) = entry.raw_frames.get(&seq) {
                        cumulative_samples += pkt.dur as u64;
                    } else {
                        break;
                    }
                }
                let sample_rate = entry.meta.codec_params.sample_rate;
                let elapsed_us = party_now - entry.start_party_time;
                let elapsed_samples = elapsed_us * sample_rate as u64 / 1_000_000;
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
                    frames_played,
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
            let sample_rate = entry.meta.codec_params.sample_rate;

            let current_seq = if entry.playing && party_now > entry.start_party_time {
                let elapsed_us = party_now - entry.start_party_time;
                let elapsed_samples = elapsed_us * sample_rate as u64 / 1_000_000;
                let mut cumulative = 0u64;
                let mut seq = entry.start_seq;
                while let Some(pkt) = entry.raw_frames.get(&seq) {
                    if cumulative >= elapsed_samples {
                        break;
                    }
                    cumulative += pkt.dur as u64;
                    seq += 1;
                }
                seq
            } else {
                entry.start_seq
            };

            let end_check = (current_seq + 200).min(entry.meta.total_frames);

            for seq in 1..=end_check {
                if !entry.raw_frames.contains_key(&seq) {
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
