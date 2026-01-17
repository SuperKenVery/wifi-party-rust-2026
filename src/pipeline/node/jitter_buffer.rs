//! A jitter buffer for AudioFrame with sequence number handling and adaptive latency.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.
//! It supports partial reads and adaptive latency management.

use super::{Sink, Source};
use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crossbeam::atomic::AtomicCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tracing::debug;

const BASE_LATENCY_THRESHOLD: u64 = 4;
const MAX_LATENCY_THRESHOLD: u64 = 32;
const LATENCY_WINDOW_SIZE: usize = 32;
const STABILITY_THRESHOLD: f64 = 0.7;
const EMA_ALPHA: f64 = 0.1;

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
    latency_window: Mutex<LatencyWindow>,
    stability: AtomicU64,
    latency_ema: AtomicU64,
    audio_level_ema: AtomicU64,
    expected_frame_size: AtomicU64,
}

struct LatencyWindow {
    buffer: [u64; LATENCY_WINDOW_SIZE],
    index: usize,
    count: usize,
}

impl LatencyWindow {
    fn new() -> Self {
        Self {
            buffer: [u64::MAX; LATENCY_WINDOW_SIZE],
            index: 0,
            count: 0,
        }
    }

    fn record(&mut self, latency: u64) {
        self.buffer[self.index] = latency;
        self.index = (self.index + 1) % LATENCY_WINDOW_SIZE;
        if self.count < LATENCY_WINDOW_SIZE {
            self.count += 1;
        }
    }

    fn min(&self) -> u64 {
        if self.count == 0 {
            return 0;
        }
        self.buffer[..self.count.min(LATENCY_WINDOW_SIZE)]
            .iter()
            .copied()
            .min()
            .unwrap_or(0)
    }

    fn reset(&mut self, latency: u64) {
        self.buffer = [latency; LATENCY_WINDOW_SIZE];
        self.index = 1;
        self.count = 1;
    }
}

impl JitterBufferStats {
    fn new() -> Self {
        Self {
            latency_window: Mutex::new(LatencyWindow::new()),
            stability: AtomicU64::new(STABILITY_THRESHOLD.to_bits()),
            latency_ema: AtomicU64::new(0f64.to_bits()),
            audio_level_ema: AtomicU64::new(0f64.to_bits()),
            expected_frame_size: AtomicU64::new(0),
        }
    }

    fn min_latency(&self) -> u64 {
        self.latency_window.lock().unwrap().min()
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

    fn record_latency(&self, latency: u64) {
        self.latency_window.lock().unwrap().record(latency);

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

    fn reset_latency(&self, latency: u64) {
        self.latency_window.lock().unwrap().reset(latency);
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

    fn latency_threshold(&self) -> u64 {
        let stability = self.stability().max(0.25);
        ((BASE_LATENCY_THRESHOLD as f64 / stability) as u64).min(MAX_LATENCY_THRESHOLD)
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
            debug!(
                "Read slot fail: wanted seq={}, stored={:?}",
                expected_seq,
                self.stored_seq()
            );
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
const RESET_THRESHOLD_COUNT: u64 = 50;
const RESET_THRESHOLD_DIFF: u64 = 100;

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
    pub fn skip(&self, amount: u64) {
        self.read_seq.fetch_add(amount, Ordering::AcqRel);
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

        // Drain partial frame first
        let needed = len - collected.len();
        collected.extend(partial.take(needed));

        // Record latency once per pull (before consuming frames)
        let latency = self.latency();
        self.stats.record_latency(latency);

        // Adaptive catch-up: skip frames when min latency exceeds threshold
        // Use minimum latency to find the best achievable latency, not average
        // Threshold grows when stability is low (bad network)
        let threshold = self.stats.latency_threshold();
        let min_latency = self.stats.min_latency();
        if min_latency > threshold * 2 {
            let would_skip = min_latency - threshold;
            debug!(
                "JitterBuffer Pull: Catch up (min_latency={}, threshold={}, stability={:.2}), skipping {} frames",
                min_latency,
                threshold,
                self.stats.stability(),
                would_skip
            );
            self.skip(would_skip);
            self.stats.reset_latency(self.latency());
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
                    // If buffer is truly empty (nothing written ahead), return what we have
                    if self.latency() == 0 {
                        // Fill remaining with silence if we have partial data
                        if !collected.is_empty() {
                            collected.resize(len, Sample::silence());
                        }
                        break;
                    }

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

                    debug!(
                        "JitterBuffer Pull: Missing seq={} (stability={:.2}), filling {} samples with silence",
                        self.read_seq.load(Ordering::Acquire) - 1,
                        stability,
                        fill_count
                    );
                    collected.extend(std::iter::repeat(Sample::silence()).take(fill_count));

                    // Store leftover silence in partial to maintain timing
                    if frame_size > fill_count {
                        let leftover = frame_size - fill_count;
                        partial.store(
                            std::iter::repeat(Sample::silence()).take(leftover),
                            result_seq,
                        );
                    }

                    // Unstable: stop after one miss, don't try to fetch more frames
                    if stability < STABILITY_THRESHOLD {
                        break;
                    }
                }
            }
        }

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
                    self.stats.reset_latency(0);
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FRAME_LEN: usize = 960;

    #[derive(Clone)]
    struct XorShift64 {
        state: u64,
    }

    impl XorShift64 {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }

        fn next_u64(&mut self) -> u64 {
            let mut x = self.state;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.state = x;
            x
        }

        fn next_usize(&mut self) -> usize {
            self.next_u64() as usize
        }

        fn next_i16(&mut self) -> i16 {
            (self.next_u64() as u16) as i16
        }
    }

    fn make_frame(seq: u64) -> AudioFrame<i16, 2, 48000> {
        AudioFrame::new(seq, vec![0i16; TEST_FRAME_LEN]).unwrap()
    }

    fn make_frame_with_data(seq: u64, data: Vec<i16>) -> AudioFrame<i16, 2, 48000> {
        AudioFrame::new(seq, data).unwrap()
    }

    #[test]
    fn test_basic_push_pull() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 0);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 1);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 2);
        assert!(buffer.pull(TEST_FRAME_LEN).is_none());
    }

    #[test]
    fn test_out_of_order() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(2));
        buffer.push(make_frame(0));
        buffer.push(make_frame(1));

        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 0);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 1);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 2);
    }

    #[test]
    fn test_duplicate_ignored() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        buffer.push(make_frame(0));
        buffer.push(make_frame(0));

        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 0);
        assert!(buffer.pull(TEST_FRAME_LEN).is_none());
    }

    #[test]
    fn test_late_packet_ignored() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        buffer.push(make_frame(1));

        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 0);

        buffer.push(make_frame(0));

        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 1);
        assert!(buffer.pull(TEST_FRAME_LEN).is_none());
    }

    #[test]
    fn test_hole_fills_silence() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        // Hole at seq 0 - fills with silence and skips to seq 1
        let frame = buffer.pull(TEST_FRAME_LEN).unwrap();
        assert!(frame.samples.data().iter().all(|&s| s == 0));

        // Now we get seq 1 and 2
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 1);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 2);
    }

    #[test]
    fn test_skip_hole() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        // Manual skip past the hole
        buffer.skip(1);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 1);
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 2);
    }

    #[test]
    fn test_overwrite_old_data() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        assert_eq!(buffer.pull(TEST_FRAME_LEN).unwrap().sequence_number, 0);

        for i in 1..=7 {
            buffer.push(make_frame(i));
        }

        buffer.push(make_frame(9));

        assert_eq!(buffer.latency(), 8);
    }

    #[test]
    fn test_latency_tracking() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        assert_eq!(buffer.latency(), 0);

        buffer.push(make_frame(0));
        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        assert_eq!(buffer.latency(), 2);

        buffer.pull(TEST_FRAME_LEN);
        assert_eq!(buffer.latency(), 1);

        buffer.pull(TEST_FRAME_LEN);
        buffer.pull(TEST_FRAME_LEN);
        assert_eq!(buffer.latency(), 0);
    }

    #[test]
    fn test_partial_read() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        let data: Vec<i16> = (0..100).collect();
        buffer.push(make_frame_with_data(0, data.clone()));

        let frame1 = buffer.pull(40).unwrap();
        assert_eq!(frame1.samples.data().len(), 40);
        assert_eq!(frame1.samples.data(), &data[0..40]);

        let frame2 = buffer.pull(40).unwrap();
        assert_eq!(frame2.samples.data().len(), 40);
        assert_eq!(frame2.samples.data(), &data[40..80]);

        // Only 20 samples left, but we requested 40 - fills remaining with silence
        let frame3 = buffer.pull(40).unwrap();
        assert_eq!(frame3.samples.data().len(), 40);
        assert_eq!(&frame3.samples.data()[..20], &data[80..100]);
        assert!(frame3.samples.data()[20..].iter().all(|&s| s == 0));

        assert!(buffer.pull(40).is_none());
    }

    #[test]
    fn test_partial_read_across_frames() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        let data1: Vec<i16> = (0..50).collect();
        let data2: Vec<i16> = (50..100).collect();
        buffer.push(make_frame_with_data(0, data1));
        buffer.push(make_frame_with_data(1, data2));

        let frame = buffer.pull(80).unwrap();
        assert_eq!(frame.samples.data().len(), 80);
        let expected: Vec<i16> = (0..80).collect();
        assert_eq!(frame.samples.data(), &expected[..]);
    }

    #[test]
    fn test_catch_up() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(114514));

        for _ in 1..100 {
            let _ = buffer.pull(960);
        }

        assert!(buffer.read_seq.load(Ordering::Acquire) <= 114514 + 100);
    }

    #[test]
    fn test_catch_up_resets_latency() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.write_seq.store(100, Ordering::Release);
        buffer.read_seq.store(0, Ordering::Release);

        let _ = buffer.collect_samples(TEST_FRAME_LEN);

        assert!(buffer.stats.min_latency() <= buffer.stats.latency_threshold());
    }

    #[test]
    fn test_randomized_roundtrip() {
        let mut rng = XorShift64::new(0x9E37_79B9_7F4A_7C15);

        let channels = 2usize;
        let total_samples = 50_000usize - (50_000usize % channels);
        let mut original: Vec<i16> = Vec::with_capacity(total_samples);
        for _ in 0..total_samples {
            original.push(rng.next_i16());
        }

        let max_write_chunk = 1_200usize;
        let mut write_chunks: Vec<Vec<i16>> = Vec::new();
        let mut write_idx = 0usize;
        while write_idx < original.len() {
            let remaining = original.len() - write_idx;
            let max_frames = (max_write_chunk / channels).max(1);
            let remaining_frames = remaining / channels;
            let frames = 1 + (rng.next_usize() % max_frames.min(remaining_frames));
            let chunk_len = frames * channels;
            write_chunks.push(original[write_idx..write_idx + chunk_len].to_vec());
            write_idx += chunk_len;
        }

        let buffer = JitterBuffer::<i16, 2, 48000>::new(64);
        let mut next_chunk = 0usize;
        let mut next_seq = 0u64;
        let mut pushed_samples = 0usize;
        let mut read_samples = 0usize;
        let mut output: Vec<i16> = Vec::with_capacity(original.len());

        let max_read_len = 1_000usize;
        while read_samples < original.len() {
            while next_chunk < write_chunks.len() && buffer.latency() < BASE_LATENCY_THRESHOLD {
                let data = write_chunks[next_chunk].clone();
                pushed_samples += data.len();
                buffer.push(make_frame_with_data(next_seq, data));
                next_seq += 1;
                next_chunk += 1;
            }

            let queued = pushed_samples - read_samples;
            if queued == 0 {
                continue;
            }

            let max_read_frames = (max_read_len / channels).max(1);
            let queued_frames = queued / channels;
            let frames = 1 + (rng.next_usize() % max_read_frames.min(queued_frames));
            let read_len = frames * channels;
            let frame = buffer.pull(read_len).unwrap();
            let got = frame.samples.data();
            output.extend(got.iter().copied());
            read_samples += got.len();
        }

        assert_eq!(output, original);
    }
}
