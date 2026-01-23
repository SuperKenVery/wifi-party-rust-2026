//! A jitter buffer for AudioFrame with target-latency based adaptive control.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.
//! It supports partial reads and adaptive latency management.
//!
//! Key design principle: assume missing packets are lost, not delayed.
//! - On push: clamp read_seq forward if it falls outside target latency window
//! - On pull: only hold back when read_seq would exceed write_seq (underrun)
//! - Adapt target latency: increase on high loss, decrease when min latency stays high

use crate::audio::effects::calculate_rms_level;
use crate::audio::frame::AudioFrame;
use crate::audio::AudioSample;
use crate::pipeline::{Sink, Source};
use crossbeam::atomic::AtomicCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use tracing::debug;

const EMA_ALPHA: f64 = 0.01;
const RESET_THRESHOLD_COUNT: u64 = 50;
const RESET_THRESHOLD_DIFF: u64 = 100;

const DEFAULT_TARGET_LATENCY: u64 = 3; // frames (~60ms at 20ms/frame)
const MIN_TARGET_LATENCY: u64 = 1;
const MAX_TARGET_LATENCY: u64 = 25; // ~500ms at 20ms/frame

const LATENCY_WINDOW_SIZE: usize = 50; // sliding window for min latency detection
const HIGH_MIN_LATENCY_THRESHOLD: u64 = 5; // if min latency stays above this, decrease target
const HIGH_LOSS_THRESHOLD: f64 = 0.05; // 10% loss rate triggers target increase
const LOW_LOSS_THRESHOLD: f64 = 0.02; // 2% loss rate allows target decrease

const SNAPSHOT_WINDOW_SIZE: usize = 200; // ~1 second at ~5ms/pull (256 samples @ 48kHz)

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

/// A snapshot of the jitter buffer state at a single pull operation.
/// Used for debugging visualization of buffer behavior over time.
#[derive(Debug, Clone, PartialEq)]
pub struct PullSnapshot {
    pub write_seq: u64,
    pub read_seq: u64,
    /// Status of slots between read_seq and write_seq.
    /// Index 0 = read_seq, index N = read_seq + N.
    /// true = slot has data, false = slot is empty (missing packet).
    pub slot_status: Vec<bool>,
}

/// Statistics for jitter buffer behavior.
pub struct JitterBufferStats {
    expected_frame_size: AtomicU64,
    loss_rate_ema: AtomicU64,
    target_latency: AtomicU64,
    latency_window: Mutex<VecDeque<u64>>,
    audio_level: AtomicU32,
    snapshots: Mutex<VecDeque<PullSnapshot>>,
}

impl JitterBufferStats {
    fn new() -> Self {
        Self {
            expected_frame_size: AtomicU64::new(0),
            loss_rate_ema: AtomicU64::new(0f64.to_bits()),
            target_latency: AtomicU64::new(DEFAULT_TARGET_LATENCY),
            latency_window: Mutex::new(VecDeque::with_capacity(LATENCY_WINDOW_SIZE)),
            audio_level: AtomicU32::new(0),
            snapshots: Mutex::new(VecDeque::with_capacity(SNAPSHOT_WINDOW_SIZE)),
        }
    }

    /// Returns the expected frame size in samples (total, not per channel).
    pub fn expected_frame_size(&self) -> u64 {
        self.expected_frame_size.load(Ordering::Acquire)
    }

    /// Returns the current loss rate EMA (0.0 to 1.0).
    pub fn loss_rate(&self) -> f64 {
        f64::from_bits(self.loss_rate_ema.load(Ordering::Acquire))
    }

    /// Returns the current target latency in frames.
    pub fn target_latency(&self) -> u64 {
        self.target_latency.load(Ordering::Acquire)
    }

    /// Returns the current audio level (0-100).
    pub fn audio_level(&self) -> u32 {
        self.audio_level.load(Ordering::Acquire)
    }

    /// Returns a copy of recent pull snapshots (last ~1 second).
    pub fn recent_snapshots(&self) -> Vec<PullSnapshot> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots.iter().cloned().collect()
    }

    fn record_latency(&self, latency: u64) {
        let mut window = self.latency_window.lock().unwrap();
        if window.len() >= LATENCY_WINDOW_SIZE {
            window.pop_front();
        }
        window.push_back(latency);
    }

    fn min_latency_in_window(&self) -> Option<u64> {
        let window = self.latency_window.lock().unwrap();
        window.iter().copied().min()
    }

    fn record_expected_frame_size(&self, size: u64) {
        self.expected_frame_size
            .compare_exchange(0, size, Ordering::AcqRel, Ordering::Relaxed)
            .ok();
    }

    fn record_hit(&self) {
        let curr = self.loss_rate();
        let new_val = (1.0 - EMA_ALPHA) * curr;
        self.loss_rate_ema
            .store(new_val.to_bits(), Ordering::Release);
    }

    fn record_miss(&self) {
        let curr = self.loss_rate();
        let new_val = (1.0 - EMA_ALPHA) * curr + EMA_ALPHA;
        self.loss_rate_ema
            .store(new_val.to_bits(), Ordering::Release);
    }

    fn record_audio_level(&self, level: u32) {
        self.audio_level.store(level, Ordering::Release);
    }

    fn record_snapshot(&self, snapshot: PullSnapshot) {
        let mut snapshots = self.snapshots.lock().unwrap();
        if snapshots.len() >= SNAPSHOT_WINDOW_SIZE {
            snapshots.pop_front();
        }
        snapshots.push_back(snapshot);
    }

    fn adjust_target_latency(&self) {
        let loss_rate = self.loss_rate();
        let current_target = self.target_latency.load(Ordering::Acquire);

        // Increase target latency when loss is high
        if loss_rate > HIGH_LOSS_THRESHOLD && current_target < MAX_TARGET_LATENCY {
            let new_target = (current_target + 1).min(MAX_TARGET_LATENCY);
            self.target_latency.store(new_target, Ordering::Release);
            debug!(
                "JitterBuffer: Target latency increased {} -> {} (loss_rate={:.2}%)",
                current_target,
                new_target,
                loss_rate * 100.0
            );
            return;
        }

        // Decrease target latency when loss is low and min latency is high
        if loss_rate < LOW_LOSS_THRESHOLD && current_target > MIN_TARGET_LATENCY {
            if let Some(min_lat) = self.min_latency_in_window() {
                if min_lat >= HIGH_MIN_LATENCY_THRESHOLD {
                    let new_target = (current_target - 1).max(MIN_TARGET_LATENCY);
                    self.target_latency.store(new_target, Ordering::Release);
                    debug!(
                        "JitterBuffer: Target latency decreased {} -> {} (min_latency={})",
                        current_target, new_target, min_lat
                    );
                }
            }
        }
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
/// # Design Principle
/// Assume missing packets are lost, not delayed. This avoids waiting for packets
/// that will never arrive, which would otherwise enlarge silence gaps.
///
/// # Behavior
/// - On push: clamp read_seq forward if it falls outside target latency window
/// - On pull: only hold back when read_seq would exceed write_seq (underrun)
/// - Adapt target latency: increase on high loss, decrease when min latency stays high
pub struct JitterBuffer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    slots: Box<[Slot<Sample, CHANNELS, SAMPLE_RATE>]>,
    capacity: usize,
    /// Next sequence number to read (will-be-read).
    /// The reader attempts to fetch from slot[read_seq % capacity].
    read_seq: CachePadded<AtomicU64>,
    /// Highest sequence number written (last-written).
    /// Updated when a new frame with seq > write_seq is pushed.
    write_seq: CachePadded<AtomicU64>,
    /// Counter for detecting host restart.
    /// Incremented when receiving packets with seq far below read_seq.
    /// When count reaches RESET_THRESHOLD_COUNT, we assume the sender restarted
    /// and reset the buffer to accept the new sequence range.
    late_packet_count: AtomicU64,
    stats: JitterBufferStats,
    /// A partially read frame. Here we store its left over for next pull's use.
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
    pub fn latency(&self) -> u64 {
        let write_seq = self.write_seq.load(Ordering::Acquire);
        let read_seq = self.read_seq.load(Ordering::Acquire);
        write_seq.saturating_sub(read_seq)
    }

    /// Returns a reference to the jitter buffer statistics.
    pub fn stats(&self) -> &JitterBufferStats {
        &self.stats
    }

    /// Captures the current slot status between read_seq and write_seq.
    fn capture_slot_status(&self, read_seq: u64, write_seq: u64) -> Vec<bool> {
        let count = write_seq.saturating_sub(read_seq) as usize;
        (0..count)
            .map(|i| {
                let seq = read_seq + i as u64;
                let slot_idx = self.slot_index(seq);
                let slot = &self.slots[slot_idx];
                slot.stored_seq().map_or(false, |s| s == seq)
            })
            .collect()
    }

    /// Clamp read_seq forward to stay within target latency of write_seq.
    fn clamp_read_seq(&self, write_seq: u64) {
        let target_latency = self.stats.target_latency();
        let desired_read_seq = write_seq.saturating_sub(target_latency);

        loop {
            let current_read_seq = self.read_seq.load(Ordering::Acquire);
            if current_read_seq >= desired_read_seq {
                return;
            }

            if self
                .read_seq
                .compare_exchange_weak(
                    current_read_seq,
                    desired_read_seq,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                debug!(
                    "JitterBuffer: Clamped read_seq from {} to {} (write_seq={}, target_latency={})",
                    current_read_seq, desired_read_seq, write_seq, target_latency
                );
                return;
            }
            debug!("clamp_read_seq: Spinning to update read_seq");
        }
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

        let needed = len - collected.len();
        collected.extend(partial.take(needed));

        let latency = self.latency();
        self.stats.record_latency(latency);
        self.stats.adjust_target_latency();

        while collected.len() < len {
            let read_seq = self.read_seq.load(Ordering::Acquire);
            let write_seq = self.write_seq.load(Ordering::Acquire);

            // Underrun: read_seq > write_seq, hold back (don't advance)
            // Note: read_seq == write_seq means we have exactly one frame at write_seq to read
            if read_seq > write_seq {
                debug!(
                    "JitterBuffer: Underrun, read_seq={} > write_seq={}, holding back",
                    read_seq, write_seq
                );
                // self.stats.record_miss();
                let remaining = len - collected.len();
                collected.extend(std::iter::repeat(Sample::silence()).take(remaining));
                break;
            }

            match self.try_fetch_frame() {
                Some(frame) => {
                    self.skip(1);
                    self.stats.record_hit();
                    result_seq = frame.sequence_number;

                    let samples = frame.samples.into_inner();
                    let needed = len - collected.len();

                    if samples.len() <= needed {
                        collected.extend(samples);
                    } else {
                        collected.extend(samples[..needed].iter().copied());
                        partial.store(samples[needed..].iter().copied(), result_seq);
                    }
                }
                None => {
                    // Slot is empty - check if this is a hole (missing packet) or underrun
                    // Hole: read_seq < write_seq, packet was lost
                    // Underrun: read_seq == write_seq, we're caught up to writer
                    if read_seq >= write_seq {
                        debug!(
                            "JitterBuffer: Underrun (empty slot), read_seq={} >= write_seq={}, holding back",
                            read_seq, write_seq
                        );
                        let remaining = len - collected.len();
                        collected.extend(std::iter::repeat(Sample::silence()).take(remaining));
                        break;
                    }

                    // Missing packet - assume lost, skip immediately
                    self.stats.record_miss();
                    self.skip(1);

                    let remaining = len - collected.len();
                    let frame_size = self.stats.expected_frame_size() as usize;
                    let fill_count = if frame_size > 0 {
                        frame_size.min(remaining)
                    } else {
                        remaining
                    };

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

        if collected.is_empty() {
            None
        } else {
            let read_seq = self.read_seq.load(Ordering::Acquire);
            let write_seq = self.write_seq.load(Ordering::Acquire);
            let slot_status = self.capture_slot_status(read_seq, write_seq);
            self.stats.record_snapshot(PullSnapshot {
                write_seq,
                read_seq,
                slot_status,
            });

            let level = calculate_rms_level(&collected);
            self.stats.record_audio_level(level);

            Some((collected, result_seq))
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        let frame_size = input.samples.data().len() as u64;
        self.stats.record_expected_frame_size(frame_size);

        let seq = input.sequence_number;
        let slot_idx = self.slot_index(seq);
        let slot = &self.slots[slot_idx];

        let mut read_seq = self.read_seq.load(Ordering::Acquire);
        let write_seq = self.write_seq.load(Ordering::Acquire);

        if read_seq == 0 && write_seq == 0 && seq > 0 {
            self.read_seq
                .compare_exchange(0, seq, Ordering::AcqRel, Ordering::Relaxed)
                .ok();
            read_seq = self.read_seq.load(Ordering::Acquire);
        }

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
                    // let mut partial = self.partial.lock().unwrap();
                    // *partial = PartialFrameState::new();
                }
            }
            return;
        }

        self.late_packet_count.store(0, Ordering::Release);

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

        let mut new_write_seq = write_seq;
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
                new_write_seq = seq;
                break;
            }
            debug!("push: Spinning to update write_seq");
        }

        // Clamp read_seq forward if outside target latency window
        self.clamp_read_seq(new_write_seq);
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let (samples, seq) = self.collect_samples(len).unwrap();

        debug_assert_eq!(
            samples.len(),
            len,
            "JitterBuffer: collected {} samples but expected {}",
            samples.len(),
            len
        );

        AudioFrame::new(seq, samples).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::frame::AudioFrame;

    type TestBuffer = JitterBuffer<f32, 2, 48000>;
    type TestFrame = AudioFrame<f32, 2, 48000>;

    fn make_frame(seq: u64, len: usize) -> TestFrame {
        let samples: Vec<f32> = (0..len)
            .map(|i| (seq as f32) + (i as f32) * 0.001)
            .collect();
        TestFrame::new(seq, samples).unwrap()
    }

    #[test]
    fn test_push_and_pull_single_frame() {
        let buffer = TestBuffer::new(16);
        let frame = make_frame(1, 1920);

        buffer.push(frame);

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some());
        let pulled = pulled.unwrap();
        assert_eq!(pulled.samples.data().len(), 1920);
        assert_eq!(pulled.sequence_number, 1);
    }

    #[test]
    fn test_pull_exact_length() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));
        buffer.push(make_frame(2, 1920));

        let pulled = buffer.pull(1000);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().samples.data().len(), 1000);

        let pulled = buffer.pull(500);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().samples.data().len(), 500);
    }

    #[test]
    fn test_pull_across_frames() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));
        buffer.push(make_frame(2, 1920));

        let pulled = buffer.pull(2500);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().samples.data().len(), 2500);
    }

    #[test]
    fn test_pull_with_underrun_fills_silence() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().samples.data().len(), 1920);

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some());
        let data = pulled.unwrap();
        assert_eq!(data.samples.data().len(), 1920);
        assert!(data.samples.data().iter().all(|&s| s == 0.0));
    }

    #[test]
    fn test_out_of_order_frames() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));
        buffer.push(make_frame(3, 1920));
        buffer.push(make_frame(2, 1920));

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().sequence_number, 1);

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().sequence_number, 2);

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some());
        assert_eq!(pulled.unwrap().sequence_number, 3);
    }

    #[test]
    fn test_partial_frame_handling() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));

        let pulled1 = buffer.pull(1000);
        assert!(pulled1.is_some());
        assert_eq!(pulled1.unwrap().samples.data().len(), 1000);

        let pulled2 = buffer.pull(920);
        assert!(pulled2.is_some());
        assert_eq!(pulled2.unwrap().samples.data().len(), 920);
    }

    #[test]
    fn test_multiple_push_pull_cycles() {
        let buffer = TestBuffer::new(16);

        for seq in 1..=10 {
            buffer.push(make_frame(seq, 1920));
            let pulled = buffer.pull(1920);
            assert!(pulled.is_some());
            assert_eq!(pulled.unwrap().samples.data().len(), 1920);
        }
    }

    #[test]
    fn test_pull_length_always_matches_request() {
        let buffer = TestBuffer::new(16);

        for seq in 1..=5 {
            buffer.push(make_frame(seq, 1920));
        }

        let test_lengths = [100, 500, 1920, 2000, 3000, 5000];
        for &len in &test_lengths {
            let pulled = buffer.pull(len);
            assert!(pulled.is_some(), "pull({}) returned None", len);
            assert_eq!(
                pulled.unwrap().samples.data().len(),
                len,
                "pull({}) returned wrong length",
                len
            );
        }
    }

    #[test]
    fn test_pull_with_gaps_maintains_length() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));
        buffer.push(make_frame(3, 1920));
        buffer.push(make_frame(5, 1920));

        let test_lengths = [100, 500, 1920, 2500, 4000];
        for &len in &test_lengths {
            let pulled = buffer.pull(len);
            assert!(pulled.is_some(), "pull({}) returned None", len);
            let actual_len = pulled.unwrap().samples.data().len();
            assert_eq!(
                actual_len, len,
                "pull({}) returned {} samples instead",
                len, actual_len
            );
        }
    }

    #[test]
    fn test_pull_empty_buffer_returns_silence() {
        let buffer = TestBuffer::new(16);

        let pulled = buffer.pull(1920);
        assert!(pulled.is_some(), "Empty buffer should return silence");
        let data = pulled.unwrap().samples.into_inner();
        assert_eq!(data.len(), 1920);
        assert!(
            data.iter().all(|&x| x == 0.0),
            "Empty buffer should return all zeros"
        );
    }

    #[test]
    fn test_continuous_push_pull_data_integrity() {
        let buffer = TestBuffer::new(16);

        for seq in 1..=10u64 {
            let samples: Vec<f32> = (0..1920)
                .map(|i| (seq as f32) + (i as f32) * 0.0001)
                .collect();
            let frame = TestFrame::new(seq, samples).unwrap();
            buffer.push(frame);
        }

        let target_latency = buffer.stats.target_latency();
        let expected_start_seq = 10u64.saturating_sub(target_latency);
        let available_frames = (10 - expected_start_seq + 1) as usize;

        let mut all_pulled: Vec<f32> = Vec::new();
        for _ in 0..available_frames {
            let pulled = buffer.pull(1920);
            assert!(pulled.is_some());
            all_pulled.extend(pulled.unwrap().samples.into_inner());
        }

        assert_eq!(all_pulled.len(), 1920 * available_frames);

        let non_zero_count = all_pulled.iter().filter(|&&x| x != 0.0).count();
        assert!(
            non_zero_count > all_pulled.len() / 2,
            "Too many zeros in pulled data: {} zeros out of {}",
            all_pulled.len() - non_zero_count,
            all_pulled.len()
        );
    }

    #[test]
    fn test_read_seq_clamping_on_large_jump() {
        let buffer = TestBuffer::new(32);

        buffer.push(make_frame(1, 1920));

        let read_seq_before = buffer.read_seq.load(Ordering::Acquire);
        assert_eq!(read_seq_before, 1);

        buffer.push(make_frame(100, 1920));

        let read_seq_after = buffer.read_seq.load(Ordering::Acquire);
        let target_latency = buffer.stats.target_latency();
        let expected_min_read = 100u64.saturating_sub(target_latency);
        assert!(
            read_seq_after >= expected_min_read,
            "read_seq {} should be >= {} (write_seq 100 - target_latency {})",
            read_seq_after,
            expected_min_read,
            target_latency
        );
    }

    #[test]
    fn test_no_holdback_on_gaps() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));
        buffer.push(make_frame(5, 1920));

        let target_latency = buffer.stats.target_latency();
        let expected_read_seq = 5u64.saturating_sub(target_latency);

        let pulled1 = buffer.pull(1920);
        assert!(pulled1.is_some());

        if expected_read_seq <= 1 {
            assert_eq!(pulled1.unwrap().sequence_number, 1);
        }

        let pulled2 = buffer.pull(1920);
        assert!(pulled2.is_some());

        let pulled3 = buffer.pull(1920);
        assert!(pulled3.is_some());
    }

    #[test]
    fn test_underrun_holds_back() {
        let buffer = TestBuffer::new(16);

        buffer.push(make_frame(1, 1920));

        let pulled1 = buffer.pull(1920);
        assert!(pulled1.is_some());
        assert_eq!(pulled1.unwrap().sequence_number, 1);

        let read_before = buffer.read_seq.load(Ordering::Acquire);

        let pulled2 = buffer.pull(1920);
        assert!(pulled2.is_some());
        let data = pulled2.unwrap();
        assert!(data.samples.data().iter().all(|&s| s == 0.0));

        let read_after = buffer.read_seq.load(Ordering::Acquire);
        assert_eq!(
            read_before, read_after,
            "read_seq should not advance on underrun"
        );
    }
}
