//! A lock-free jitter buffer for AudioFrame with sequence number handling.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.

use super::{Sink, Source};
use crate::audio::frame::AudioFrame;
use crate::audio::AudioSample;
use crossbeam::atomic::AtomicCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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

/// A lock-free jitter buffer that reorders out-of-order frames by sequence number.
///
/// Implements both [`Sink`] for receiving frames and [`Source`] for retrieving them
/// in sequence order. The buffer is safe to use across threads - one thread can push
/// while another pulls.
///
/// # Behavior
/// - Out-of-order frames are held until earlier frames arrive or are skipped
/// - Duplicate frames (same sequence number) are ignored
/// - Late frames (sequence < current read position) are dropped
/// - Use [`skip()`](Self::skip) to skip missing frames and avoid stalling
pub struct JitterBuffer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    slots: Box<[Slot<Sample, CHANNELS, SAMPLE_RATE>]>,
    capacity: usize,
    read_seq: CachePadded<AtomicU64>,
    write_seq: CachePadded<AtomicU64>,
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
        }
    }

    fn slot_index(&self, seq: u64) -> usize {
        (seq % self.capacity as u64) as usize
    }

    /// Skip the current expected frame.
    ///
    /// Use this when a frame is missing and you want to continue playback
    /// rather than waiting indefinitely.
    pub fn skip(&self) {
        self.read_seq.fetch_add(1, Ordering::AcqRel);
    }

    /// Returns the number of frames buffered ahead of the read position.
    pub fn latency(&self) -> u64 {
        let write_seq = self.write_seq.load(Ordering::Acquire);
        let read_seq = self.read_seq.load(Ordering::Acquire);
        write_seq.saturating_sub(read_seq)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        let seq = input.sequence_number;
        let slot_idx = self.slot_index(seq);
        let slot = &self.slots[slot_idx];

        // Drop late packets
        if seq < self.read_seq.load(Ordering::Acquire) {
            return;
        }

        // Drop duplicates
        if let Some(previous_written_seq) = slot.stored_seq()
            && previous_written_seq >= seq
        {
            return;
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
                break;
            }
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let read_seq = self.read_seq.load(Ordering::Acquire);
        let slot_idx = self.slot_index(read_seq);
        let slot = &self.slots[slot_idx];

        let frame = slot.take(read_seq)?;
        self.read_seq.store(read_seq + 1, Ordering::Release);
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(seq: u64) -> AudioFrame<i16, 2, 48000> {
        AudioFrame::new(seq, vec![0i16; 960]).unwrap()
    }

    #[test]
    fn test_basic_push_pull() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        assert_eq!(buffer.pull().unwrap().sequence_number, 0);
        assert_eq!(buffer.pull().unwrap().sequence_number, 1);
        assert_eq!(buffer.pull().unwrap().sequence_number, 2);
        assert!(buffer.pull().is_none());
    }

    #[test]
    fn test_out_of_order() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(2));
        buffer.push(make_frame(0));
        buffer.push(make_frame(1));

        assert_eq!(buffer.pull().unwrap().sequence_number, 0);
        assert_eq!(buffer.pull().unwrap().sequence_number, 1);
        assert_eq!(buffer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_duplicate_ignored() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        buffer.push(make_frame(0));
        buffer.push(make_frame(0));

        assert_eq!(buffer.pull().unwrap().sequence_number, 0);
        assert!(buffer.pull().is_none());
    }

    #[test]
    fn test_late_packet_ignored() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        buffer.push(make_frame(1));

        assert_eq!(buffer.pull().unwrap().sequence_number, 0);

        buffer.push(make_frame(0));

        assert_eq!(buffer.pull().unwrap().sequence_number, 1);
        assert!(buffer.pull().is_none());
    }

    #[test]
    fn test_hole_returns_none() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        assert!(buffer.pull().is_none());

        buffer.push(make_frame(0));
        assert_eq!(buffer.pull().unwrap().sequence_number, 0);
        assert_eq!(buffer.pull().unwrap().sequence_number, 1);
        assert_eq!(buffer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_skip_hole() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(1));
        buffer.push(make_frame(2));

        assert!(buffer.pull().is_none());
        buffer.skip();
        assert_eq!(buffer.pull().unwrap().sequence_number, 1);
        assert_eq!(buffer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_overwrite_old_data() {
        let buffer = JitterBuffer::<i16, 2, 48000>::new(8);

        buffer.push(make_frame(0));
        assert_eq!(buffer.pull().unwrap().sequence_number, 0);

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

        buffer.pull();
        assert_eq!(buffer.latency(), 1);

        buffer.pull();
        buffer.pull();
        assert_eq!(buffer.latency(), 0);
    }
}
