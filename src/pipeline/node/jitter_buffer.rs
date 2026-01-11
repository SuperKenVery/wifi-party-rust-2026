//! A lock-free jitter buffer for AudioFrame with sequence number handling.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.

use super::{Sink, Source};
use crate::audio::frame::AudioFrame;
use crate::audio::AudioSample;
use crossbeam::atomic::AtomicCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
    has_data: std::sync::atomic::AtomicBool,
    stored_seq: AtomicU64,
    data: AtomicCell<Option<Box<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>>>>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Slot<Sample, CHANNELS, SAMPLE_RATE> {
    fn new() -> Self {
        Self {
            has_data: std::sync::atomic::AtomicBool::new(false),
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

struct JitterBufferInner<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    slots: Box<[Slot<Sample, CHANNELS, SAMPLE_RATE>]>,
    capacity: usize,
    read_seq: CachePadded<AtomicU64>,
    write_seq: CachePadded<AtomicU64>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    JitterBufferInner<Sample, CHANNELS, SAMPLE_RATE>
{
    fn new(capacity: usize) -> Self {
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
}

pub struct JitterBufferProducer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    inner: Arc<JitterBufferInner<Sample, CHANNELS, SAMPLE_RATE>>,
}

pub struct JitterBufferConsumer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    inner: Arc<JitterBufferInner<Sample, CHANNELS, SAMPLE_RATE>>,
}

pub fn jitter_buffer<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
    capacity: usize,
) -> (
    JitterBufferProducer<Sample, CHANNELS, SAMPLE_RATE>,
    JitterBufferConsumer<Sample, CHANNELS, SAMPLE_RATE>,
) {
    let inner = Arc::new(JitterBufferInner::new(capacity));
    let producer = JitterBufferProducer {
        inner: inner.clone(),
    };
    let consumer = JitterBufferConsumer { inner };
    (producer, consumer)
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for JitterBufferProducer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        let seq = input.sequence_number;
        let slot_idx = self.inner.slot_index(seq);
        let slot = &self.inner.slots[slot_idx];

        if seq < self.inner.read_seq.load(Ordering::Acquire) {
            return;
        }

        if let Some(previous_written_seq) = slot.stored_seq()
            && previous_written_seq >= seq
        {
            return;
        }

        slot.write(seq, input);

        loop {
            let current_write_seq = self.inner.write_seq.load(Ordering::Acquire);
            if seq <= current_write_seq {
                break;
            }
            if self
                .inner
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
    for JitterBufferConsumer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let read_seq = self.inner.read_seq.load(Ordering::Acquire);
        let slot_idx = self.inner.slot_index(read_seq);
        let slot = &self.inner.slots[slot_idx];

        let frame = slot.take(read_seq)?;
        self.inner.read_seq.store(read_seq + 1, Ordering::Release);
        Some(frame)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    JitterBufferConsumer<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn skip(&self) {
        self.inner.read_seq.fetch_add(1, Ordering::AcqRel);
    }

    pub fn next_expected_seq(&self) -> u64 {
        self.inner.read_seq.load(Ordering::Acquire)
    }

    pub fn latency(&self) -> u64 {
        let write_seq = self.inner.write_seq.load(Ordering::Acquire);
        let read_seq = self.inner.read_seq.load(Ordering::Acquire);
        write_seq.saturating_sub(read_seq)
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
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(0));
        producer.push(make_frame(1));
        producer.push(make_frame(2));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_out_of_order() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(2));
        producer.push(make_frame(0));
        producer.push(make_frame(1));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_duplicate_ignored() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(0));
        producer.push(make_frame(0));
        producer.push(make_frame(0));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_late_packet_ignored() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(0));
        producer.push(make_frame(1));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);

        producer.push(make_frame(0));

        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_hole_returns_none() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(1));
        producer.push(make_frame(2));

        assert!(consumer.pull().is_none());

        producer.push(make_frame(0));
        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_skip_hole() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(1));
        producer.push(make_frame(2));

        assert!(consumer.pull().is_none());
        consumer.skip();
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_overwrite_old_data() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        producer.push(make_frame(0));
        assert_eq!(consumer.pull().unwrap().sequence_number, 0);

        for i in 1..=7 {
            producer.push(make_frame(i));
        }

        producer.push(make_frame(9));

        assert_eq!(consumer.latency(), 8);
    }

    #[test]
    fn test_latency_tracking() {
        let (producer, consumer) = jitter_buffer::<i16, 2, 48000>(8);

        assert_eq!(consumer.latency(), 0);

        producer.push(make_frame(0));
        producer.push(make_frame(1));
        producer.push(make_frame(2));

        assert_eq!(consumer.latency(), 2);

        consumer.pull();
        assert_eq!(consumer.latency(), 1);

        consumer.pull();
        consumer.pull();
        assert_eq!(consumer.latency(), 0);
    }
}
