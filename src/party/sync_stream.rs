//! Synchronized audio stream (with buffering) for music playback.
//!
//! Unlike realtime streams that play immediately, synced streams buffer audio
//! and play at a specified party clock time, ensuring all participants hear
//! the same audio at the same moment.
//!
//! # Architecture (push-based pipeline)
//!
//! ```text
//! Network packets → BufferEntry (reassembles fragments, sequences)
//!                     ↓ push(CompressedPacket)
//!                   SymphoniaDecoder (per-channel f32 PCM)
//!                     ↓ push(DecodedAudio)
//!                   FftResampler (resamples or passes through; per-channel f32 PCM)
//!                     ↓ push(DecodedAudio)
//!                   Interleaver (interleaved AudioBuffer)
//!                     ↓ push(AudioBuffer)
//!                   SimpleBuffer (pre-decoded audio ring)
//!                     ↑ pull()
//!                   SyncedAudioStreamManager::pull_and_mix() (NTP-timed playback)
//!                     ↑ pull()
//!                   Output Mixer → Speaker
//! ```
//!
//! Decoding and resampling happen eagerly on packet arrival (in the network
//! receive path). The audio callback only reads from the pre-decoded
//! SimpleBuffer, ensuring deterministic low-latency playback.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use symphonia::core::codecs::DecoderOptions;
use tracing::{error, info, warn};

use crate::audio::AudioSample;
use crate::audio::buffers::simple_buffer::SimpleBuffer;
use crate::audio::decoders::{CompressedPacket, FftResampler, Interleaver, PacketCounter, SymphoniaDecoder};
use crate::audio::frame::AudioBuffer;
use crate::audio::symphonia_compat::WireCodecParams;
use crate::pipeline::{GraphNode, Pullable, Pushable};

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
///
/// Large payloads (e.g. ALAC frames of 3–8 KB) exceed the link MTU, so we
/// split them across multiple `SyncedFrame`s that share the same
/// `sequence_number` but differ in `fragment_idx`. Receivers reassemble by
/// seq before decoding. Frames that fit in a single datagram use
/// `fragment_idx = 0, fragment_total = 1`.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedFrame {
    pub stream_id: SyncedStreamId,
    pub sequence_number: u64,
    pub dur: u32,
    pub fragment_idx: u16,
    pub fragment_total: u16,
    pub data: Vec<u8>,
}

/// Max bytes of compressed audio per fragment. Leaves headroom under a
/// conservative 1400-byte UDP payload for the rest of `SyncedFrame`,
/// the `NetworkPacket::Synced` wrapper, and rkyv framing overhead.
pub const MAX_FRAGMENT_DATA: usize = 1200;

/// Raw packet stored for sender-side retransmission.
#[derive(Clone)]
pub struct RawPacket {
    pub dur: u32,
    pub data: Vec<u8>,
}

impl SyncedFrame {
    /// Build a non-fragmented frame (`fragment_total = 1`).
    pub fn whole(stream_id: SyncedStreamId, sequence_number: u64, dur: u32, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            sequence_number,
            dur,
            fragment_idx: 0,
            fragment_total: 1,
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

/// Collects fragments of a single logical `SyncedFrame` (same seq) until
/// complete. Cleared on seek or stream teardown; otherwise it just stays
/// until the full set arrives (possibly via retransmission).
struct FragmentSet {
    total: u16,
    received_count: u16,
    dur: u32,
    parts: Vec<Option<Vec<u8>>>,
}

/// Control commands for synced streams.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub enum SyncedControl {
    Start {
        stream_id: SyncedStreamId,
        party_clock_time: u64,
        seq: u64,
    },
    Pause {
        stream_id: SyncedStreamId,
    },
}

// ---------------------------------------------------------------------------
//  BufferEntry — per-source/stream state using push-based pipeline
// ---------------------------------------------------------------------------

/// A buffer for a single stream from a single source.
///
/// Compressed packets are pushed through the decode pipeline eagerly on
/// arrival (in `receive()`). Decoded PCM accumulates in `output_buffer`,
/// which the audio callback reads from via `pull_and_mix()`.
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    // -- Push pipeline --
    /// Head of the push pipeline. Push CompressedPacket here to decode eagerly.
    pipeline_head: Arc<dyn Pushable<CompressedPacket>>,
    /// Pre-decoded audio buffer. Pull from here in the audio callback.
    output_buffer: SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>,
    /// Resets decoder, resampler internal state, and output buffer on seek.
    pipeline_reset: Box<dyn Fn() + Send + Sync>,
    /// Packet progress counters (for UI).
    packet_counter: PacketCounter,

    meta: SyncedStreamMeta,
    /// Out of order compressed frames waiting for predecessors.
    pending_raw: HashMap<u64, SyncedFrame>,
    /// Long frames segmented to fit MTU, here we store segments.
    pending_fragments: HashMap<u64, FragmentSet>,
    /// Next expected sequence number for feeding into the pipeline.
    next_feed_seq: u64,
    last_seen: Instant,

    // -- Playback state --
    playing: bool,
    /// Party clock time (µs) when playback should start / resumed.
    start_party_time: u64,
    /// Total samples pulled to output (for progress UI).
    samples_played: u64,
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

        // Metadata update for existing entry.
        if let Some(mut entry) = self.buffers.get_mut(&key) {
            entry.meta = meta;
            entry.last_seen = Instant::now();
            return;
        }

        // New stream — create decoder and wire up push pipeline.
        let Some(decoder) = create_decoder(&meta.codec_params) else {
            return;
        };

        let output_buffer = SimpleBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new();
        let output_buffer_sink: Arc<SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>> =
            Arc::new(output_buffer);

        let decoder_node = Arc::new(SymphoniaDecoder::<CHANNELS>::new(decoder));

        // Build push pipeline: decoder → resampler → interleaver → output_buffer.
        // FftResampler passes through unchanged when src rate == SAMPLE_RATE.
        let resampler_node = match FftResampler::<CHANNELS, SAMPLE_RATE>::new(meta.codec_params.sample_rate) {
            Ok(r) => Arc::new(r),
            Err(e) => {
                error!("Failed to create resampler for stream {}: {}", meta.stream_id, e);
                return;
            }
        };

        let interleaver_node = Arc::new(Interleaver::<Sample, CHANNELS, SAMPLE_RATE>::new());

        // Wire: decoder_graph → resampler_graph → interleaver_graph → output_buffer_sink
        let interleaver_graph = Arc::new(GraphNode::new(interleaver_node));
        interleaver_graph.add_output(output_buffer_sink.clone());
        let resampler_graph = Arc::new(GraphNode::new(resampler_node.clone()));
        resampler_graph.add_output(interleaver_graph);
        let decoder_graph = Arc::new(GraphNode::new(decoder_node.clone()));
        decoder_graph.add_output(resampler_graph);

        let reset_dec = decoder_node.clone();
        let reset_res = resampler_node.clone();
        let reset_buf = output_buffer_sink.clone();
        let pipeline_head: Arc<dyn Pushable<CompressedPacket>> = decoder_graph;
        let pipeline_reset: Box<dyn Fn() + Send + Sync> = Box::new(move || {
            reset_dec.reset();
            reset_res.reset();
            reset_buf.reset();
        });

        info!(
            "Creating synced buffer for source {} stream {}",
            source_addr, meta.stream_id
        );

        // Deref the Arc to get a SimpleBuffer clone for pulling.
        // SimpleBuffer uses Arc<Mutex<...>> internally so clones share state.
        let output_for_pull = (*output_buffer_sink).clone();

        self.buffers.insert(
            key,
            BufferEntry {
                pipeline_head,
                output_buffer: output_for_pull,
                pipeline_reset,
                packet_counter: PacketCounter::new(),
                meta,
                pending_raw: HashMap::new(),
                pending_fragments: HashMap::new(),
                next_feed_seq: 1,
                last_seen: Instant::now(),
                playing: false,
                start_party_time: 0,
                samples_played: 0,
            },
        );
    }

    /// Receives playback control.
    pub fn receive_control(&self, source_addr: SocketAddr, control: SyncedControl) {
        let stream_id = match &control {
            SyncedControl::Start { stream_id, .. } => *stream_id,
            SyncedControl::Pause { stream_id } => *stream_id,
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
            SyncedControl::Start {
                party_clock_time,
                seq,
                ..
            } => {
                entry.playing = true;
                entry.start_party_time = party_clock_time;
                entry.last_seen = Instant::now();

                // Reset pipeline and pending queues on seek.
                // Sender handles seeking in the source file — receiver just
                // starts fresh from the new seq. For resume (seq == next_feed_seq),
                // reset is harmless since no packets are queued ahead.
                if seq != entry.next_feed_seq {
                    (entry.pipeline_reset)();
                    entry.next_feed_seq = seq;
                    entry.pending_raw.clear();
                    entry.pending_fragments.clear();
                }

                info!(
                    "Stream {:?} starting at seq {} at party time {}",
                    key, seq, party_clock_time
                );
            }
            SyncedControl::Pause { .. } => {
                entry.playing = false;
                entry.last_seen = Instant::now();
                info!("Stream {:?} paused", key);
            }
        }
    }

    /// Receives audio frame. Only stores if entry exists.
    ///
    /// Compressed packets are pushed through the decode pipeline in sequence
    /// order. Out-of-order packets wait in `pending_raw` until predecessors
    /// arrive. Fragmented frames are reassembled before pushing.
    pub fn receive(&self, source_addr: SocketAddr, frame: SyncedFrame) {
        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        // Collect packets to push, then release the DashMap lock before
        // pushing through the decode pipeline. Decode + resample is expensive
        // and must not block pull_and_mix (which needs iter_mut over the map).
        let (pipeline_head, packets_to_push) = {
            let Some(mut entry) = self.buffers.get_mut(&key) else {
                return;
            };
            let entry = &mut *entry;

            entry.last_seen = Instant::now();

            let seq = frame.sequence_number;

            // Duplicate or old frame.
            if seq < entry.next_feed_seq || entry.pending_raw.contains_key(&seq) {
                return;
            }

            // Reassemble fragments if needed.
            let frame = if frame.fragment_total <= 1 {
                frame
            } else {
                match Self::insert_fragment(entry, frame) {
                    Some(assembled) => assembled,
                    None => return,
                }
            };

            let mut packets = Vec::new();

            // Collect ready packets in sequence order.
            if seq == entry.next_feed_seq {
                entry.packet_counter.record_packet(seq);
                packets.push(CompressedPacket { dur: frame.dur, data: frame.data });
                entry.next_feed_seq += 1;

                // Drain any consecutive pending packets.
                while let Some(pending) = entry.pending_raw.remove(&entry.next_feed_seq) {
                    entry.packet_counter.record_packet(entry.next_feed_seq);
                    packets.push(CompressedPacket { dur: pending.dur, data: pending.data });
                    entry.next_feed_seq += 1;
                }
            } else {
                entry.pending_raw.insert(seq, frame);
                return;
            }

            (entry.pipeline_head.clone(), packets)
        };
        // Lock released — push packets through decode pipeline without contention.
        for packet in packets_to_push {
            pipeline_head.push(packet);
        }
    }

    /// Insert a fragment; return the assembled whole frame once complete.
    fn insert_fragment(
        entry: &mut BufferEntry<Sample, CHANNELS, SAMPLE_RATE>,
        frame: SyncedFrame,
    ) -> Option<SyncedFrame> {
        let seq = frame.sequence_number;
        let total = frame.fragment_total;
        let idx = frame.fragment_idx;

        if total == 0 || idx >= total {
            warn!("Bad fragment for seq={}: idx={}, total={}", seq, idx, total);
            return None;
        }

        let set = entry
            .pending_fragments
            .entry(seq)
            .or_insert_with(|| FragmentSet {
                total,
                received_count: 0,
                dur: frame.dur,
                parts: vec![None; total as usize],
            });

        if set.total != total {
            warn!(
                "Fragment total mismatch for seq={} ({} vs {}), dropping",
                seq, set.total, total
            );
            return None;
        }

        let slot = &mut set.parts[idx as usize];
        if slot.is_none() {
            *slot = Some(frame.data);
            set.received_count += 1;
        }

        if set.received_count < set.total {
            return None;
        }

        let set = entry.pending_fragments.remove(&seq)?;
        let total_len: usize = set.parts.iter().flatten().map(|v| v.len()).sum();
        let mut data = Vec::with_capacity(total_len);
        for part in set.parts.into_iter().flatten() {
            data.extend_from_slice(&part);
        }

        Some(SyncedFrame::whole(entry.meta.stream_id, seq, set.dur, data))
    }

    /// Pulls samples from all streams and mixes them together.
    ///
    /// For each playing stream whose start_party_time has arrived, pulls
    /// pre-decoded PCM from its output buffer. Multiple streams are mixed.
    pub fn pull_and_mix(
        &self,
        num_frames: usize,
    ) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let party_now = (self.party_now_fn)();
        let num_samples = num_frames * CHANNELS;
        let mut mixed: Vec<i64> = vec![0i64; num_samples];
        let mut source_count = 0usize;
        let mut actual_len = 0usize;

        for mut entry in self.buffers.iter_mut() {
            if !entry.playing || entry.start_party_time > party_now {
                continue;
            }

            let Some(buf) = entry.output_buffer.pull(num_samples) else {
                continue;
            };

            source_count += 1;
            let buf_data = buf.data();
            actual_len = actual_len.max(buf_data.len());
            entry.samples_played += buf_data.len() as u64 / CHANNELS as u64;
            for (i, sample) in buf_data.iter().enumerate() {
                mixed[i] += sample.to_i64_for_mix();
            }
        }

        if source_count == 0 {
            return None;
        }

        // Only return the actual amount of audio produced, not the full requested size.
        let result: Vec<Sample> = mixed[..actual_len]
            .iter()
            .map(|s| Sample::from_i64_mixed(*s, source_count))
            .collect();
        AudioBuffer::new(result).ok()
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
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            let is_local_sender =
                entry.key().source_addr.ip().is_loopback() && entry.key().source_addr.port() == 0;

            result.push(SyncedStreamState {
                stream_id: entry.key().stream_id,
                meta: entry.meta.clone(),
                progress: SyncedStreamProgress {
                    samples_played: entry.samples_played,
                    total_samples: entry.meta.total_samples,
                    buffered_frames: entry.packet_counter.packets_pushed(),
                    is_playing: entry.playing,
                    highest_seq_received: entry.packet_counter.highest_seq(),
                },
                is_local_sender,
            });
        }

        result
    }

    /// Identifies gaps in the received packets and returns them for retransmission.
    pub fn get_missing_frames(&self) -> Vec<(SocketAddr, SyncedStreamId, Vec<u64>)> {
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            let next_feed = entry.next_feed_seq;

            // The highest seq we've actually received is the max of
            // (next_feed_seq - 1) and pending_raw keys. If pending_raw is
            // empty, everything up to next_feed_seq has been fed — no gaps.
            let Some(&highest) = entry.pending_raw.keys().max() else {
                continue;
            };

            // Scan gaps between next_feed_seq and the highest pending packet.
            let mut missing = Vec::new();
            for seq in next_feed..highest {
                if !entry.pending_raw.contains_key(&seq) {
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

    /// Starts the background cleanup task.
    ///
    /// Must be called from within a Tokio runtime context.
    pub fn start_cleanup_task(self: &Arc<Self>, shutdown: Arc<AtomicBool>) {
        let stream = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            while !shutdown.load(Ordering::Relaxed) {
                interval.tick().await;
                stream.cleanup_stale();
            }
        });
    }

    /// Starts the background retransmit request task.
    ///
    /// Periodically checks for missing frames and sends retransmission requests.
    /// Must be called from within a Tokio runtime context.
    pub fn start_retransmit_task(
        self: &Arc<Self>,
        sender: crate::io::NetworkSender,
        shutdown: Arc<AtomicBool>,
    ) {
        use crate::party::realtime_stream::NetworkPacket;
        use crate::pipeline::Pushable;

        let stream = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(200));
            while !shutdown.load(Ordering::Relaxed) {
                interval.tick().await;
                for (_addr, stream_id, seqs) in stream.get_missing_frames() {
                    sender.push(NetworkPacket::RequestFrames { stream_id, seqs });
                }
            }
        });
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>
    for SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>
{
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let num_frames = len / CHANNELS;
        self.pull_and_mix(num_frames)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>
    for Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>
{
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let num_frames = len / CHANNELS;
        self.pull_and_mix(num_frames)
    }
}
