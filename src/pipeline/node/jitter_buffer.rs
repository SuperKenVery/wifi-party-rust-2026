//! A jitter buffer for AudioFrame with sequence number handling and adaptive latency.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.
//! It supports partial reads and adaptive latency management.

use super::{Sink, Source};
use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crossbeam::atomic::AtomicCell;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use tracing::debug;

const HIGH_STABILITY: f64 = 0.99;
const TARGET_STABILITY: f64 = 0.95;
const EMA_ALPHA: f64 = 0.01;
const RESET_THRESHOLD_COUNT: u64 = 50;
const RESET_THRESHOLD_DIFF: u64 = 100;
const HUGE_GAP_THRESHOLD: u64 = 150; // ~3 seconds at 20ms/frame
const TIMELINE_CAPACITY: usize = 500; // ~10 seconds at 20ms/frame

#[derive(Debug, Clone, PartialEq)]
pub enum JitterEvent {
    LowStabilityHoldBack { stability: f64, latency: i64 },
    MissingSeq { seq: u64, stability: f64 },
    HugeGapSkip { latency: i64, skip_amount: i64 },
    HighStabilityBump { stability: f64, latency: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    pub timestamp_ms: u64,
    pub read_seq: u64,
    pub write_seq: u64,
    pub buffer_state: Vec<bool>,
    pub event: Option<JitterEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineSnapshot {
    pub entries: Vec<TimelineEntry>,
    pub now_ms: u64,
}

#[repr(align(64))]
struct CachePadded<T>(T);

impl<T> CachePadded<T> {
    fn new(val: T) -> Self {
        Self(val)
    }
}

impl<T> std::ops::Deref for CachePadded<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Statistics for jitter buffer behavior.
pub struct JitterBufferStats {
    stability: AtomicU64,
    latency_ema: AtomicU64,
    audio_level_ema: AtomicU64,
    expected_frame_size: AtomicU64,
    timeline: Mutex<TimelineState>,
}

struct TimelineState {
    entries: VecDeque<TimelineEntry>,
    start_time: Instant,
}

impl JitterBufferStats {
    fn new() -> Self {
        Self {
            stability: AtomicU64::new(1.0f64.to_bits()),
            latency_ema: AtomicU64::new(0f64.to_bits()),
            audio_level_ema: AtomicU64::new(0f64.to_bits()),
            expected_frame_size: AtomicU64::new(0),
            timeline: Mutex::new(TimelineState {
                entries: VecDeque::with_capacity(TIMELINE_CAPACITY),
                start_time: Instant::now(),
            }),
        }
    }

    /// Returns the stability (hit rate) as EMA. Higher = more reliable delivery.
    /// Miss rate = 1.0 - stability.
    pub fn stability(&self) -> f64 {
        f64::from_bits(self.stability.load(Ordering::Acquire))
    }

    /// Returns the EMA of jitter buffer latency in frames.
    pub fn latency_ema(&self) -> f64 {
        f64::from_bits(self.latency_ema.load(Ordering::Acquire))
    }

    /// Returns the EMA of audio level (0.0 to 1.0).
    pub fn audio_level_ema(&self) -> f64 {
        f64::from_bits(self.audio_level_ema.load(Ordering::Acquire))
    }

    /// Returns the expected frame size in samples (total, not per channel).
    pub fn expected_frame_size(&self) -> u64 {
        self.expected_frame_size.load(Ordering::Acquire)
    }

    fn record_latency(&self, latency: i64) {
        let curr = self.latency_ema();
        let new_val = (1.0 - EMA_ALPHA) * curr + EMA_ALPHA * (latency as f64);
        self.latency_ema.store(new_val.to_bits(), Ordering::Release);
    }

    fn record_audio_level(&self, level: f64) {
        let curr = self.audio_level_ema();
        let new_val = (1.0 - EMA_ALPHA) * curr + EMA_ALPHA * level;
        self.audio_level_ema
            .store(new_val.to_bits(), Ordering::Release);
    }

    fn record_expected_frame_size(&self, size: u64) {
        self.expected_frame_size
            .compare_exchange(0, size, Ordering::AcqRel, Ordering::Relaxed)
            .ok();
    }

    fn record_hit(&self) {
        let curr = self.stability();
        let new_val = (1.0 - EMA_ALPHA) * curr + EMA_ALPHA;
        self.stability.store(new_val.to_bits(), Ordering::Release);
    }

    fn record_miss(&self) {
        let curr = self.stability();
        let new_val = (1.0 - EMA_ALPHA) * curr;
        self.stability.store(new_val.to_bits(), Ordering::Release);
    }

    fn record_timeline(
        &self,
        read_seq: u64,
        write_seq: u64,
        buffer_state: Vec<bool>,
        event: Option<JitterEvent>,
    ) {
        let mut state = self.timeline.lock().unwrap();
        let timestamp_ms = state.start_time.elapsed().as_millis() as u64;

        if state.entries.len() >= TIMELINE_CAPACITY {
            state.entries.pop_front();
        }

        state.entries.push_back(TimelineEntry {
            timestamp_ms,
            read_seq,
            write_seq,
            buffer_state,
            event,
        });
    }

    pub fn timeline_snapshot(&self) -> TimelineSnapshot {
        let state = self.timeline.lock().unwrap();
        let now_ms = state.start_time.elapsed().as_millis() as u64;
        let cutoff = now_ms.saturating_sub(1000);

        let entries = state
            .entries
            .iter()
            .filter(|e| e.timestamp_ms >= cutoff)
            .cloned()
            .collect();

        TimelineSnapshot { entries, now_ms }
    }
}

struct Slot<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    has_data: AtomicBool,
    stored_seq: AtomicU64,
    data: AtomicCell<Option<Box<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>>>>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Slot<Sample, CHANNELS, SAMPLE_RATE> {
    fn new() -> Self {
        Self {
            has_data: AtomicBool::new(false),
            stored_seq: AtomicU64::new(0),
            data: AtomicCell::new(None),
        }
    }

    fn write(&self, seq: u64, frame: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        self.data.swap(Some(Box::new(frame)));
        self.stored_seq.store(seq, Ordering::Release);
        self.has_data.store(true, Ordering::Release);
    }

    fn stored_seq(&self) -> Option<u64> {
        if self.has_data.load(Ordering::Acquire) {
            Some(self.stored_seq.load(Ordering::Acquire))
        } else {
            None
        }
    }

    fn take(&self, expected_seq: u64) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        if self.stored_seq()? != expected_seq {
            return None;
        }
        self.has_data.store(false, Ordering::Release);
        self.data.swap(None).map(|b| *b)
    }
}

/// Leftover samples from a partially consumed frame.
struct PartialFrameState<Sample> {
    samples: Vec<Sample>,
    offset: usize,
    seq: u64,
}

impl<Sample> PartialFrameState<Sample> {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
            offset: 0,
            seq: 0,
        }
    }

    fn take(&mut self, count: usize) -> impl Iterator<Item = Sample> + '_
    where
        Sample: Copy,
    {
        let start = self.offset;
        let end = (self.offset + count).min(self.samples.len());
        self.offset = end;
        self.samples[start..end].iter().copied()
    }

    fn store(&mut self, samples: impl Iterator<Item = Sample>, seq: u64) {
        self.samples.clear();
        self.samples.extend(samples);
        self.offset = 0;
        self.seq = seq;
    }
}

/// A jitter buffer that reorders out-of-order frames by sequence number.
///
/// Implements both [`Sink`] for receiving frames and [`Source`] for retrieving them
/// in sequence order. The buffer is safe to use across threads - one thread can push
/// while another pulls.
///
/// # Features
/// - **Partial reads**: `pull(len)` returns exactly `len` samples, storing remainder
/// - **Adaptive latency**: Skips frames when latency is too high
/// - **Stability tracking**: Waits for missing frames when connection is unstable
///
/// # Behavior
/// - Out-of-order frames are held until earlier frames arrive or are skipped
/// - Duplicate frames (same sequence number) are ignored
/// - Late frames (sequence < current read position) are dropped
pub struct JitterBuffer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    slots: Box<[Slot<Sample, CHANNELS, SAMPLE_RATE>]>,
    capacity: usize,
    read_seq: CachePadded<AtomicU64>,
    write_seq: CachePadded<AtomicU64>,
    late_packet_count: AtomicU64,
    stats: JitterBufferStats,
    partial: Mutex<PartialFrameState<Sample>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(capacity: usize) -> Self {
        let slots: Vec<Slot<Sample, CHANNELS, SAMPLE_RATE>> =
            (0..capacity).map(|_| Slot::new()).collect();
        Self {
            slots: slots.into_boxed_slice(),
            capacity,
            read_seq: CachePadded::new(AtomicU64::new(0)),
            write_seq: CachePadded::new(AtomicU64::new(0)),
            late_packet_count: AtomicU64::new(0),
            stats: JitterBufferStats::new(),
            partial: Mutex::new(PartialFrameState::new()),
        }
    }

    fn slot_index(&self, seq: u64) -> usize {
        (seq % self.capacity as u64) as usize
    }

    /// Skip the current expected frame.
    ///
    /// Use this when a frame is missing and you want to continue playback
    /// rather than waiting indefinitely.
    pub fn skip(&self, amount: i64) {
        self.read_seq.fetch_add(amount as u64, Ordering::AcqRel);
    }

    /// Returns the number of frames buffered ahead of the read position.
    pub fn latency(&self) -> i64 {
        let write_seq = self.write_seq.load(Ordering::Acquire);
        let read_seq = self.read_seq.load(Ordering::Acquire);
        (write_seq.wrapping_sub(read_seq)) as i64
    }

    /// Returns the buffer state: for each seq from read_seq to write_seq,
    /// true if that slot has data, false if missing.
    fn buffer_state(&self) -> Vec<bool> {
        let read_seq = self.read_seq.load(Ordering::Acquire);
        let write_seq = self.write_seq.load(Ordering::Acquire);
        if write_seq <= read_seq {
            return vec![];
        }
        (read_seq..=write_seq)
            .map(|seq| {
                let slot_idx = self.slot_index(seq);
                let slot = &self.slots[slot_idx];
                slot.stored_seq() == Some(seq)
            })
            .collect()
    }

    /// Returns a reference to the jitter buffer statistics.
    pub fn stats(&self) -> &JitterBufferStats {
        &self.stats
    }

    /// Try to fetch the frame at current read_seq from slots.
    /// Returns the frame if available, None otherwise.
    /// Does NOT advance read_seq - caller is responsible for that.
    fn try_fetch_frame(&self) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let read_seq = self.read_seq.load(Ordering::Acquire);
        let slot_idx = self.slot_index(read_seq);
        let slot = &self.slots[slot_idx];
        slot.take(read_seq)
    }

    /// Collect samples into the output buffer, handling partial frames and fetching new frames.
    fn collect_samples(&self, len: usize) -> Option<(Vec<Sample>, u64)> {
        let mut partial = self.partial.lock().unwrap();
        let mut collected: Vec<Sample> = Vec::with_capacity(len);
        let mut result_seq = partial.seq;
        let mut event: Option<JitterEvent> = None;

        // Capture buffer state BEFORE consuming any frames
        let snapshot_read_seq = self.read_seq.load(Ordering::Acquire);
        let snapshot_write_seq = self.write_seq.load(Ordering::Acquire);
        let snapshot_buffer_state = self.buffer_state();

        // Drain partial frame first
        let needed = len - collected.len();
        collected.extend(partial.take(needed));

        // Record latency once per pull (before consuming frames)
        let latency = self.latency();
        self.stats.record_latency(latency);

        let stability = self.stats.stability();

        // 1. Huge Gap Detection (Remote Start / Jump Forward)
        // If latency is absurdly high, remote probably started before us - jump forward
        if latency > HUGE_GAP_THRESHOLD as i64 {
            let skip_amount = latency - 1;
            if skip_amount > 0 {
                debug!(
                    "JitterBuffer: Huge latency detected ({}), skipping {} frames",
                    latency, skip_amount
                );
                event = Some(JitterEvent::HugeGapSkip {
                    latency,
                    skip_amount,
                });
                self.skip(skip_amount);
            }
        }

        // Re-read latency after potential skip
        let latency = self.latency();

        // 2. Control: Hold Back when stability is low
        // If stability drops below threshold, don't advance read_seq to let buffer build up
        if stability < TARGET_STABILITY && latency > 0 {
            debug!(
                "JitterBuffer: Low stability ({:.4}), latency={}, holding back.",
                stability, latency
            );
            event = Some(JitterEvent::LowStabilityHoldBack { stability, latency });
            let remaining = len - collected.len();
            collected.extend(std::iter::repeat(Sample::silence()).take(remaining));
            self.stats.record_hit();

            self.stats.record_timeline(
                snapshot_read_seq,
                snapshot_write_seq,
                snapshot_buffer_state,
                event,
            );

            return Some((collected, result_seq));
        }

        // 3. Control: Bump Forward when stability is very high
        // If stability is excellent and we have excess buffer, reduce latency
        if stability > HIGH_STABILITY && latency > 2 {
            debug!(
                "JitterBuffer: High stability ({:.4}), latency={}, bumping forward",
                stability, latency
            );
            event = Some(JitterEvent::HighStabilityBump { stability, latency });
            self.skip(1);
        }

        // Fetch frames until we have enough samples
        while collected.len() < len {
            match self.try_fetch_frame() {
                Some(frame) => {
                    self.skip(1);
                    self.stats.record_hit();
                    result_seq = frame.sequence_number;

                    let samples = frame.samples.into_inner();

                    // Calculate and record audio level (RMS normalized to 0.0-1.0)
                    if !samples.is_empty() {
                        let sum_sq: f64 = samples
                            .iter()
                            .map(|s| {
                                let normalized = s.to_f64_normalized();
                                normalized * normalized
                            })
                            .sum();
                        let rms = (sum_sq / samples.len() as f64).sqrt();
                        self.stats.record_audio_level(rms.min(1.0));
                    }

                    let needed = len - collected.len();

                    if samples.len() <= needed {
                        collected.extend(samples);
                    } else {
                        collected.extend(samples[..needed].iter().copied());
                        partial.store(samples[needed..].iter().copied(), result_seq);
                    }
                }
                None => {
                    let current_read = self.read_seq.load(Ordering::Acquire);
                    let current_write = self.write_seq.load(Ordering::Acquire);

                    if current_read >= current_write {
                        // Underrun - Hold Back (don't advance read_seq)
                        // This lets write_seq get ahead, building buffer
                        let remaining = len - collected.len();
                        collected.extend(std::iter::repeat(Sample::silence()).take(remaining));
                        break;
                    }

                    // Hole - Skip (Never Wait for missing packets)
                    self.skip(1);
                    self.stats.record_miss();

                    let stability = self.stats.stability();
                    let remaining = len - collected.len();
                    let frame_size = self.stats.expected_frame_size() as usize;
                    let fill_count = if frame_size > 0 {
                        frame_size.min(remaining)
                    } else {
                        remaining
                    };

                    event = Some(JitterEvent::MissingSeq {
                        seq: current_read,
                        stability,
                    });

                    collected.extend(std::iter::repeat(Sample::silence()).take(fill_count));

                    if frame_size > fill_count {
                        let leftover = frame_size - fill_count;
                        partial.store(
                            std::iter::repeat(Sample::silence()).take(leftover),
                            result_seq,
                        );
                    }
                }
            }
        }

        self.stats.record_timeline(
            snapshot_read_seq,
            snapshot_write_seq,
            snapshot_buffer_state,
            event,
        );

        if collected.is_empty() {
            None
        } else {
            Some((collected, result_seq))
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        // Record expected frame size from first push
        let frame_size = input.samples.data().len() as u64;
        self.stats.record_expected_frame_size(frame_size);

        let seq = input.sequence_number;
        let slot_idx = self.slot_index(seq);
        let slot = &self.slots[slot_idx];

        // Drop late packets (but allow some tolerance for out-of-order)
        let read_seq = self.read_seq.load(Ordering::Acquire);
        if seq < read_seq {
            let diff = read_seq - seq;
            if diff > RESET_THRESHOLD_DIFF {
                let count = self.late_packet_count.fetch_add(1, Ordering::AcqRel) + 1;
                if count >= RESET_THRESHOLD_COUNT {
                    debug!(
                        "Host restart detected: {} consecutive late packets (seq={}, read_seq={}), resetting",
                        count, seq, read_seq
                    );
                    self.read_seq.store(0, Ordering::Release);
                    self.write_seq.store(0, Ordering::Release);
                    self.late_packet_count.store(0, Ordering::Release);
                    let mut partial = self.partial.lock().unwrap();
                    *partial = PartialFrameState::new();
                }
            }
            return;
        }

        self.late_packet_count.store(0, Ordering::Release);

        // Drop duplicates - only reject if the slot has a newer or equal sequence
        if let Some(previous_written_seq) = slot.stored_seq() {
            if previous_written_seq >= seq {
                debug!(
                    "Slot already has seq {} >= incoming seq {}, dropping",
                    previous_written_seq, seq
                );
                return;
            }
        }

        slot.write(seq, input);

        // Update write_seq to track the highest sequence number seen
        loop {
            let current_write_seq = self.write_seq.load(Ordering::Acquire);
            if seq <= current_write_seq {
                break;
            }
            if self
                .write_seq
                .compare_exchange_weak(current_write_seq, seq, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                // if seq % 10 == 0 || true {
                //     debug!("Updated write_seq to {}", seq);
                // }
                break;
            }
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let (samples, seq) = self.collect_samples(len)?;

        AudioFrame::new(seq, samples).ok()
    }
}
