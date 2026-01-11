//! A lock-free jitter buffer for AudioFrame with sequence number handling.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.

use super::{Sink, Source};
use crate::audio::frame::AudioFrame;
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

struct Slot {
    has_data: std::sync::atomic::AtomicBool,
    stored_seq: AtomicU64,
    data: AtomicCell<Option<Box<AudioFrame>>>,
}

impl Slot {
    fn new() -> Self {
        Self {
            has_data: std::sync::atomic::AtomicBool::new(false),
            stored_seq: AtomicU64::new(0),
            data: AtomicCell::new(None),
        }
    }

    fn write(&self, seq: u64, frame: AudioFrame) {
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

    fn take(&self, expected_seq: u64) -> Option<AudioFrame> {
        if self.stored_seq()? != expected_seq {
            return None;
        }
        self.has_data.store(false, Ordering::Release);
        self.data.swap(None).map(|b| *b)
    }
}

struct JitterBufferInner {
    slots: Box<[Slot]>,
    capacity: usize,
    read_seq: CachePadded<AtomicU64>,
    write_seq: CachePadded<AtomicU64>,
}

impl JitterBufferInner {
    fn new(capacity: usize) -> Self {
        let slots: Vec<Slot> = (0..capacity).map(|_| Slot::new()).collect();
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

pub struct JitterBufferProducer {
    inner: Arc<JitterBufferInner>,
}

/// Single-consumer end of the jitter buffer.
/// 
/// This type is `Sync` to allow wrapping in `Arc` for use in audio callbacks,
/// but it is designed for single-consumer use. Only one thread should call
/// `pull()` at a time - concurrent calls may result in missed frames.
pub struct JitterBufferConsumer {
    inner: Arc<JitterBufferInner>,
}

pub fn jitter_buffer(capacity: usize) -> (JitterBufferProducer, JitterBufferConsumer) {
    let inner = Arc::new(JitterBufferInner::new(capacity));
    let producer = JitterBufferProducer {
        inner: inner.clone(),
    };
    let consumer = JitterBufferConsumer { inner };
    (producer, consumer)
}

impl Sink for JitterBufferProducer {
    type Input = AudioFrame;

    fn push(&self, input: AudioFrame) {
        let seq = input.sequence_number;
        let slot_idx = self.inner.slot_index(seq);
        let slot = &self.inner.slots[slot_idx];

        // Ignore if older than read position
        if seq < self.inner.read_seq.load(Ordering::Acquire) {
            return;
        }

        // Ignore if already written newer frame
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

impl Source for JitterBufferConsumer {
    type Output = AudioFrame;

    fn pull(&self) -> Option<AudioFrame> {
        let read_seq = self.inner.read_seq.load(Ordering::Acquire);
        let slot_idx = self.inner.slot_index(read_seq);
        let slot = &self.inner.slots[slot_idx];

        let frame = slot.take(read_seq)?;
        self.inner.read_seq.store(read_seq + 1, Ordering::Release);
        Some(frame)
    }
}

impl JitterBufferConsumer {
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

    fn make_frame(seq: u64) -> AudioFrame {
        AudioFrame::new(seq, vec![0i16; 960]).unwrap()
    }

    #[test]
    fn test_basic_push_pull() {
        let (producer, consumer) = jitter_buffer(8);

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
        let (producer, consumer) = jitter_buffer(8);

        producer.push(make_frame(2));
        producer.push(make_frame(0));
        producer.push(make_frame(1));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_duplicate_ignored() {
        let (producer, consumer) = jitter_buffer(8);

        producer.push(make_frame(0));
        producer.push(make_frame(0));
        producer.push(make_frame(0));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_late_packet_ignored() {
        let (producer, consumer) = jitter_buffer(8);

        producer.push(make_frame(0));
        producer.push(make_frame(1));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);

        producer.push(make_frame(0));

        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_hole_returns_none() {
        let (producer, consumer) = jitter_buffer(8);

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
        let (producer, consumer) = jitter_buffer(8);

        producer.push(make_frame(1));
        producer.push(make_frame(2));

        assert!(consumer.pull().is_none());
        consumer.skip();
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_overwrite_old_data() {
        let (producer, consumer) = jitter_buffer(8);

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
        let (producer, consumer) = jitter_buffer(8);

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
