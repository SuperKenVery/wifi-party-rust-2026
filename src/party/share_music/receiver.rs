//! Network packets → BufferEntry (reassembles fragments, sequences per track)
//!                  ├─ Original → SymphoniaDecoder → FftResampler → Interleaver → raw buffer
//!                  └─ NoVocal  → OpusDecoder → no-vocal buffer
//!                     ↑ pull()                    ↑ pull()
//!                   SyncedAudioStreamManager::pull_and_mix() selects which buffer to pull from
//!                     ↑ pull()
//!                   Output Mixer → Speaker
//! ```
//!
//! Decoding and resampling happen eagerly on packet arrival. The sender always
//! publishes both tracks; the audio callback switches between the pre-decoded
//! buffers when it receives a shared vocal-removal control event.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use dashmap::DashMap;
use symphonia::core::codecs::DecoderOptions;
use tracing::{error, info, warn};

use crate::audio::AudioSample;
use crate::audio::buffers::simple_buffer::SimpleBuffer;
use crate::audio::decoders::{
    CompressedPacket, FftResampler, Interleaver, PacketCounter, SymphoniaDecoder,
};
use crate::audio::frame::AudioBuffer;
use crate::audio::opus::{OpusDecoder, OpusPacket};
use crate::party::combinator::SynchronizedSelect;
use crate::party::network_stream::{NetworkStream, NetworkStreamContext};
use crate::party::share_music::{
    RequestFramesPayload, SyncedControl, SyncedFrame, SyncedStreamId, SyncedStreamMeta,
    SyncedStreamProgress, SyncedStreamState, SyncedTrack,
};
use crate::party::tagged_packet::{
    PacketTag, REQUEST_FRAMES_TAG, SYNCED_CONTROL_TAG, SYNCED_META_TAG, SYNCED_TAG, TaggedPacket,
};
use crate::pipeline::{Pullable, Pushable};
use crate::push_chain;
use crate::state::PartyViewState;

const SYNCED_STREAM_TIMEOUT: Duration = Duration::from_secs(30);

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

// ---------------------------------------------------------------------------
//  BufferEntry — per-source/stream state using push-based pipeline
// ---------------------------------------------------------------------------

struct TrackReceiveState {
    pending_raw: HashMap<u64, SyncedFrame>,
    pending_fragments: HashMap<u64, FragmentSet>,
    next_feed_seq: u64,
    packet_counter: PacketCounter,
}

impl TrackReceiveState {
    fn new() -> Self {
        Self {
            pending_raw: HashMap::new(),
            pending_fragments: HashMap::new(),
            next_feed_seq: 1,
            packet_counter: PacketCounter::new(),
        }
    }

    fn reset_to(&mut self, seq: u64) {
        self.pending_raw.clear();
        self.pending_fragments.clear();
        self.next_feed_seq = seq;
    }
}

/// A buffer for a single stream from a single source.
///
/// The original and no-vocal tracks are received independently. Decoded PCM
/// accumulates in two output buffers; shared control events decide which buffer
/// the audio callback pulls from.
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    // -- Receiving pipeline --
    /// Head of the original-file push pipeline. Push CompressedPacket here to
    /// decode original compressed packets eagerly.
    original_pipeline_head: Arc<dyn Pushable<CompressedPacket>>,
    /// Opus decoder for sender-produced no-vocal packets.
    no_vocal_decoder: Arc<OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>>,
    /// Pull-side selector for raw/no-vocal output buffers.
    output_selector: Arc<SynchronizedSelect<Sample, CHANNELS, SAMPLE_RATE>>,
    /// Pre-decoded sender-side vocal removal buffer. Push no-vocal decoded PCM here.
    output_buffer_no_vocal: SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>,
    /// Resets decoder/resampler state and both output buffers on seek.
    reset_decoder_states: Box<dyn Fn() + Send + Sync>,

    meta: SyncedStreamMeta,
    original_track: TrackReceiveState,
    no_vocal_track: TrackReceiveState,
    last_seen: Instant,

    // -- Playback state --
    playing: bool,
    /// Party clock time (µs) when playback should start / resumed.
    start_party_time: u64,
    /// Total samples pulled to output (for progress UI).
    samples_played: u64,
    vocal_removal_active: bool,
    pending_vocal_removal: Option<(bool, u64)>,
}

enum ReadyPackets<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    Original(Arc<dyn Pushable<CompressedPacket>>, Vec<SyncedFrame>),
    NoVocal(
        Arc<OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>>,
        SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>,
        Vec<SyncedFrame>,
    ),
}

/// Manages synchronized audio streams from multiple sources.
pub struct SyncedAudioStreamManager<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffers: DashMap<BufferKey, BufferEntry<Sample, CHANNELS, SAMPLE_RATE>>,
    party_now_fn: Arc<dyn Fn() -> u64 + Send + Sync>,
    vocal_removal_enabled: Arc<AtomicBool>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new<F>(party_now_fn: F, vocal_removal_enabled: Arc<AtomicBool>) -> Self
    where
        F: Fn() -> u64 + Send + Sync + 'static,
    {
        Self {
            buffers: DashMap::new(),
            party_now_fn: Arc::new(party_now_fn),
            vocal_removal_enabled,
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

        if let Err(e) = self.handle_new_stream(source_addr, key, meta) {
            error!("Failed to create synced buffer: {e:#}");
        }
    }

    fn handle_new_stream(
        &self,
        source_addr: SocketAddr,
        key: BufferKey,
        meta: SyncedStreamMeta,
    ) -> anyhow::Result<()> {
        let stream_id = meta.stream_id;

        let decoder = symphonia::default::get_codecs()
            .make(
                &meta.codec_params.to_symphonia(),
                &DecoderOptions::default(),
            )
            .with_context(|| format!("create decoder for stream {stream_id}"))?;

        let output_buffer_raw = SimpleBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new();
        let output_buffer_raw_sink: Arc<_> = Arc::new(output_buffer_raw);

        let output_buffer_removed = SimpleBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new();
        let output_buffer_removed_sink: Arc<_> = Arc::new(output_buffer_removed);

        let decoder_node = Arc::new(SymphoniaDecoder::<CHANNELS>::new(decoder));
        let to_output_rate_node_for_raw = Arc::new(
            FftResampler::<CHANNELS, SAMPLE_RATE>::new(meta.codec_params.sample_rate)
                .with_context(|| {
                    format!("create output-rate resampler (raw) for stream {stream_id}")
                })?,
        );

        let interleaver_node_for_raw =
            Arc::new(Interleaver::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let no_vocal_decoder = Arc::new(
            OpusDecoder::<Sample, CHANNELS, SAMPLE_RATE>::new()
                .with_context(|| format!("create no-vocal Opus decoder for stream {stream_id}"))?,
        );

        // Wire: decoder → to_output_rate → interleaver → output_buffer_raw.
        // The no-vocal track arrives as Opus and is decoded directly into
        // output_buffer_removed in `receive()`.
        let original_pipeline_head: Arc<dyn Pushable<CompressedPacket>> = push_chain![
            decoder_node.clone(),
            to_output_rate_node_for_raw.clone(),
            interleaver_node_for_raw.clone(),
            => output_buffer_raw_sink.clone()
        ];

        let reset_dec = decoder_node.clone();
        let reset_to_output_rate_raw = to_output_rate_node_for_raw.clone();
        let reset_no_vocal_decoder = no_vocal_decoder.clone();
        let reset_buf_raw = output_buffer_raw_sink.clone();
        let reset_buf_removed = output_buffer_removed_sink.clone();
        let original_pipeline_reset: Box<dyn Fn() + Send + Sync> = Box::new(move || {
            reset_dec.reset();
            reset_to_output_rate_raw.reset();
            reset_no_vocal_decoder.reset();
            reset_buf_raw.reset();
            reset_buf_removed.reset();
        });

        info!(
            "Creating synced buffer for source {} stream {}",
            source_addr, stream_id
        );

        // Deref the Arc to get a SimpleBuffer clone for pulling.
        // SimpleBuffer uses Arc<Mutex<...>> internally so clones share state.
        let output_raw_for_pull: Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>> =
            Arc::new((*output_buffer_raw_sink).clone());
        let output_removed_for_pull: Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>> =
            Arc::new((*output_buffer_removed_sink).clone());
        let output_selector = Arc::new(SynchronizedSelect::new([
            output_raw_for_pull,
            output_removed_for_pull,
        ]));
        let output_removed_for_push = (*output_buffer_removed_sink).clone();

        self.buffers.insert(
            key,
            BufferEntry {
                original_pipeline_head,
                no_vocal_decoder,
                output_selector,
                output_buffer_no_vocal: output_removed_for_push,
                reset_decoder_states: original_pipeline_reset,
                meta,
                original_track: TrackReceiveState::new(),
                no_vocal_track: TrackReceiveState::new(),
                last_seen: Instant::now(),
                playing: false,
                start_party_time: 0,
                samples_played: 0,
                vocal_removal_active: false,
                pending_vocal_removal: None,
            },
        );

        Ok(())
    }

    /// Receives playback control.
    pub fn receive_control(&self, source_addr: SocketAddr, control: SyncedControl) {
        let stream_id = match &control {
            SyncedControl::Start { stream_id, .. } => *stream_id,
            SyncedControl::Pause { stream_id } => *stream_id,
            SyncedControl::SetVocalRemoval { stream_id, .. } => *stream_id,
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
                no_vocal_seq,
                ..
            } => {
                entry.playing = true;
                entry.start_party_time = party_clock_time;
                entry.last_seen = Instant::now();
                // Reset samples_played so drift correction is relative to
                // the new start_party_time, not accumulated from a prior session.
                entry.samples_played = 0;
                entry.output_selector.reset_to(0);

                // Reset pipeline and pending queues on seek.
                // Sender handles seeking in the source file — receiver just
                // starts fresh from the new seq. For resume (seq == next_feed_seq),
                // reset is harmless since no packets are queued ahead.
                if seq != entry.original_track.next_feed_seq
                    || no_vocal_seq != entry.no_vocal_track.next_feed_seq
                {
                    (entry.reset_decoder_states)();
                    entry.original_track.reset_to(seq);
                    entry.no_vocal_track.reset_to(no_vocal_seq);
                }

                info!(
                    "Stream {:?} starting at seq {} / no-vocal seq {} at party time {}",
                    key, seq, no_vocal_seq, party_clock_time
                );
            }
            SyncedControl::Pause { .. } => {
                entry.playing = false;
                entry.last_seen = Instant::now();
                info!("Stream {:?} paused", key);
            }
            SyncedControl::SetVocalRemoval {
                enabled,
                party_clock_time,
                ..
            } => {
                entry.pending_vocal_removal = Some((enabled, party_clock_time));
                entry.last_seen = Instant::now();
                self.vocal_removal_enabled.store(enabled, Ordering::Relaxed);
                info!(
                    "Stream {:?} scheduled vocal-removal={} at party time {}",
                    key, enabled, party_clock_time
                );
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
        let action = {
            let Some(mut entry) = self.buffers.get_mut(&key) else {
                return;
            };
            let entry = &mut *entry;

            entry.last_seen = Instant::now();

            match frame.track {
                SyncedTrack::Original => {
                    let ready = Self::collect_ready_frames(&mut entry.original_track, frame);
                    if ready.is_empty() {
                        return;
                    }
                    ReadyPackets::Original(entry.original_pipeline_head.clone(), ready)
                }
                SyncedTrack::NoVocal => {
                    let ready = Self::collect_ready_frames(&mut entry.no_vocal_track, frame);
                    if ready.is_empty() {
                        return;
                    }
                    ReadyPackets::NoVocal(
                        entry.no_vocal_decoder.clone(),
                        entry.output_buffer_no_vocal.clone(),
                        ready,
                    )
                }
            }
        };

        match action {
            ReadyPackets::Original(pipeline_head, frames) => {
                for frame in frames {
                    pipeline_head.push(CompressedPacket {
                        dur: frame.dur,
                        data: frame.data,
                    });
                }
            }
            ReadyPackets::NoVocal(decoder, output, frames) => {
                for frame in frames {
                    let packet = OpusPacket {
                        data: frame.data,
                        frame_size: frame.dur as usize * CHANNELS,
                    };
                    if let Some(decoded) = decoder.decode_packet(&packet) {
                        output.push(decoded);
                    }
                }
            }
        }
    }

    fn collect_ready_frames(track: &mut TrackReceiveState, frame: SyncedFrame) -> Vec<SyncedFrame> {
        let seq = frame.sequence_number;

        // Duplicate or old frame.
        if seq < track.next_feed_seq || track.pending_raw.contains_key(&seq) {
            return Vec::new();
        }

        // Reassemble fragments if needed.
        let frame = if frame.fragment_total <= 1 {
            frame
        } else {
            match Self::insert_fragment(track, frame) {
                Some(assembled) => assembled,
                None => return Vec::new(),
            }
        };

        let mut frames = Vec::new();

        // Collect ready packets in sequence order.
        if seq == track.next_feed_seq {
            track.packet_counter.record_packet(seq);
            frames.push(frame);
            track.next_feed_seq += 1;

            // Drain any consecutive pending packets.
            while let Some(pending) = track.pending_raw.remove(&track.next_feed_seq) {
                track.packet_counter.record_packet(track.next_feed_seq);
                frames.push(pending);
                track.next_feed_seq += 1;
            }
        } else {
            track.pending_raw.insert(seq, frame);
        }

        frames
    }

    /// Insert a fragment; return the assembled whole frame once complete.
    fn insert_fragment(
        track_state: &mut TrackReceiveState,
        frame: SyncedFrame,
    ) -> Option<SyncedFrame> {
        let seq = frame.sequence_number;
        let total = frame.fragment_total;
        let idx = frame.fragment_idx;
        let stream_id = frame.stream_id;
        let track = frame.track;

        if total == 0 || idx >= total {
            warn!("Bad fragment for seq={}: idx={}, total={}", seq, idx, total);
            return None;
        }

        let set = track_state
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

        let set = track_state.pending_fragments.remove(&seq)?;
        let total_len: usize = set.parts.iter().flatten().map(|v| v.len()).sum();
        let mut data = Vec::with_capacity(total_len);
        for part in set.parts.into_iter().flatten() {
            data.extend_from_slice(&part);
        }

        Some(SyncedFrame::for_track(track, stream_id, seq, set.dur, data))
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

        // 10ms lag threshold before we attempt drift correction.
        let drift_threshold = SAMPLE_RATE as u64 * 10 / 1000;

        for mut entry in self.buffers.iter_mut() {
            if let Some((enabled, switch_at)) = entry.pending_vocal_removal {
                if party_now >= switch_at {
                    entry.vocal_removal_active = enabled;
                    entry.pending_vocal_removal = None;
                    self.vocal_removal_enabled.store(enabled, Ordering::Relaxed);
                }
            }

            if !entry.playing || entry.start_party_time > party_now {
                continue;
            }

            // Drift correction relative to party clock.
            let elapsed_us = party_now.saturating_sub(entry.start_party_time);
            let expected_samples = elapsed_us * SAMPLE_RATE as u64 / 1_000_000;

            if entry.samples_played + drift_threshold < expected_samples {
                // Lagging: advance the selector's logical position. Each
                // underlying buffer discards what it has now and records any
                // remaining debt until missing packets arrive.
                let lag_samples = expected_samples - entry.samples_played;
                entry.output_selector.discard_to(expected_samples);
                warn!(
                    "Synced stream: We are lagging: discarding {:.1}ms of buffered audio",
                    lag_samples as f64 * 1000.0 / SAMPLE_RATE as f64
                );
                entry.samples_played = expected_samples;
            } else if expected_samples + drift_threshold < entry.samples_played {
                // Ahead: hold back by contributing silence this callback.
                // Don't pull and don't advance samples_played — let the party
                // clock catch up before resuming normal output.
                warn!(
                    "Synced stream: We are ahead by {:.1}ms, inserting silence",
                    (entry.samples_played - expected_samples) as f64 * 1000.0 / SAMPLE_RATE as f64
                );
                continue;
            }

            entry
                .output_selector
                .set_selected(if entry.vocal_removal_active { 1 } else { 0 });

            let Some(buf) = entry.output_selector.pull(num_samples) else {
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
                    buffered_frames: entry.original_track.packet_counter.packets_pushed(),
                    is_playing: entry.playing,
                    highest_seq_received: entry.original_track.packet_counter.highest_seq(),
                },
                is_local_sender,
            });
        }

        result
    }

    /// Identifies gaps in the received packets and returns them for retransmission.
    pub fn get_missing_frames(&self) -> Vec<(SocketAddr, SyncedStreamId, SyncedTrack, Vec<u64>)> {
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            for (track, state) in [
                (SyncedTrack::Original, &entry.original_track),
                (SyncedTrack::NoVocal, &entry.no_vocal_track),
            ] {
                let Some(missing) = Self::missing_for_track(state) else {
                    continue;
                };
                result.push((
                    entry.key().source_addr,
                    entry.key().stream_id,
                    track,
                    missing,
                ));
            }
        }
        result
    }

    fn missing_for_track(state: &TrackReceiveState) -> Option<Vec<u64>> {
        let next_feed = state.next_feed_seq;

        // The highest seq we've actually received is the max of
        // (next_feed_seq - 1) and pending_raw keys. If pending_raw is
        // empty, everything up to next_feed_seq has been fed — no gaps.
        let Some(&highest) = state.pending_raw.keys().max() else {
            return None;
        };

        // Scan gaps between next_feed_seq and the highest pending packet.
        let mut missing = Vec::new();
        for seq in next_feed..highest {
            if !state.pending_raw.contains_key(&seq) {
                missing.push(seq);
                if missing.len() >= 100 {
                    break;
                }
            }
        }

        (!missing.is_empty()).then_some(missing)
    }

    /// Starts the background cleanup task.
    ///
    /// Must be called from within a Tokio runtime context.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let stream = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                stream.cleanup_stale();
            }
        });
    }

    /// Starts the background retransmit request task.
    ///
    /// Periodically checks for missing frames and sends retransmission requests.
    /// Must be called from within a Tokio runtime context.
    pub fn start_retransmit_task(self: &Arc<Self>, sender: crate::io::NetworkSender) {
        use crate::pipeline::Pushable;

        let stream = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                for (_addr, stream_id, track, seqs) in stream.get_missing_frames() {
                    let payload = rkyv::to_bytes::<rkyv::rancor::Error>(&RequestFramesPayload {
                        stream_id,
                        track,
                        seqs,
                    })
                    .expect("RequestFramesPayload serialization")
                    .into_vec();
                    sender.push(TaggedPacket {
                        tag: REQUEST_FRAMES_TAG,
                        payload,
                    });
                }
            }
        });
    }

    pub fn start_view_task(self: &Arc<Self>, view_state: Arc<PartyViewState>) {
        let stream = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                view_state.set_synced_streams(stream.active_streams());
            }
        });
    }
}

impl<S: AudioSample, const C: usize, const SR: u32> NetworkStream<S, C, SR>
    for SyncedAudioStreamManager<S, C, SR>
{
    fn tags(&self) -> &'static [PacketTag] {
        &[SYNCED_TAG, SYNCED_META_TAG, SYNCED_CONTROL_TAG]
    }

    fn handle(&self, source: SocketAddr, tag: PacketTag, bytes: &[u8]) -> anyhow::Result<()> {
        match tag {
            SYNCED_TAG => {
                let frame = rkyv::from_bytes::<SyncedFrame, rkyv::rancor::Error>(bytes)
                    .map_err(|e| anyhow::anyhow!("SyncedFrame deserialize: {:?}", e))?;
                self.receive(source, frame);
            }
            SYNCED_META_TAG => {
                let meta = rkyv::from_bytes::<SyncedStreamMeta, rkyv::rancor::Error>(bytes)
                    .map_err(|e| anyhow::anyhow!("SyncedStreamMeta deserialize: {:?}", e))?;
                self.receive_meta(source, meta);
            }
            SYNCED_CONTROL_TAG => {
                let control = rkyv::from_bytes::<SyncedControl, rkyv::rancor::Error>(bytes)
                    .map_err(|e| anyhow::anyhow!("SyncedControl deserialize: {:?}", e))?;
                self.receive_control(source, control);
            }
            _ => unreachable!("SyncedAudioStreamManager received unexpected tag {tag}"),
        }
        Ok(())
    }

    fn start(self: Arc<Self>, ctx: NetworkStreamContext) {
        self.start_cleanup_task();
        self.start_retransmit_task(ctx.sender);
        self.start_view_task(ctx.view_state);
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
        self.as_ref().pull(len)
    }
}
