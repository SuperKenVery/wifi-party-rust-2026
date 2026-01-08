//! A lock-free jitter buffer for AudioFrame with sequence number handling.
//!
//! This buffer handles out-of-order packets, duplicates, and late arrivals
//! using a slot-based design where each slot is indexed by sequence number.

use super::{Sink, Source};
use crate::audio::frame::AudioFrame;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

const SLOT_EMPTY: u8 = 0;
const SLOT_WRITING: u8 = 1;
const SLOT_READY: u8 = 2;

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
    state: AtomicU8,
    stored_seq: AtomicU64,
    data: UnsafeCell<MaybeUninit<AudioFrame>>,
}

impl Slot {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(SLOT_EMPTY),
            stored_seq: AtomicU64::new(0),
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

unsafe impl Send for Slot {}
unsafe impl Sync for Slot {}

struct JitterBufferInner {
    slots: Box<[Slot]>,
    capacity: usize,
    read_seq: CachePadded<AtomicU64>,
}

impl JitterBufferInner {
    fn new(capacity: usize) -> Self {
        let slots: Vec<Slot> = (0..capacity).map(|_| Slot::new()).collect();
        Self {
            slots: slots.into_boxed_slice(),
            capacity,
            read_seq: CachePadded::new(AtomicU64::new(0)),
        }
    }

    fn slot_index(&self, seq: u64) -> usize {
        (seq % self.capacity as u64) as usize
    }
}

impl Drop for JitterBufferInner {
    fn drop(&mut self) {
        for slot in self.slots.iter() {
            if slot.state.load(Ordering::Acquire) == SLOT_READY {
                unsafe {
                    (*slot.data.get()).assume_init_drop();
                }
            }
        }
    }
}

pub struct JitterBufferProducer {
    inner: Arc<JitterBufferInner>,
}

pub struct JitterBufferConsumer {
    inner: Arc<JitterBufferInner>,
    cached_read_seq: u64,
}

unsafe impl Send for JitterBufferProducer {}
unsafe impl Send for JitterBufferConsumer {}

pub fn jitter_buffer(capacity: usize) -> (JitterBufferProducer, JitterBufferConsumer) {
    let inner = Arc::new(JitterBufferInner::new(capacity));
    let producer = JitterBufferProducer {
        inner: inner.clone(),
    };
    let consumer = JitterBufferConsumer {
        inner,
        cached_read_seq: 0,
    };
    (producer, consumer)
}

impl Sink for JitterBufferProducer {
    type Input = AudioFrame;

    fn push(&mut self, input: AudioFrame) {
        let seq = input.sequence_number;
        let read_seq = self.inner.read_seq.load(Ordering::Acquire);

        if seq < read_seq {
            return;
        }

        let slot_idx = self.inner.slot_index(seq);
        let slot = &self.inner.slots[slot_idx];

        loop {
            let state = slot.state.load(Ordering::Acquire);

            match state {
                SLOT_EMPTY => {
                    if slot
                        .state
                        .compare_exchange_weak(
                            SLOT_EMPTY,
                            SLOT_WRITING,
                            Ordering::AcqRel,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        unsafe {
                            (*slot.data.get()).write(input);
                        }
                        slot.stored_seq.store(seq, Ordering::Release);
                        slot.state.store(SLOT_READY, Ordering::Release);
                        return;
                    }
                }
                SLOT_READY => {
                    let stored = slot.stored_seq.load(Ordering::Acquire);
                    if stored == seq {
                        return;
                    }
                    if stored < seq && stored < read_seq {
                        if slot
                            .state
                            .compare_exchange_weak(
                                SLOT_READY,
                                SLOT_WRITING,
                                Ordering::AcqRel,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                        {
                            unsafe {
                                (*slot.data.get()).assume_init_drop();
                                (*slot.data.get()).write(input);
                            }
                            slot.stored_seq.store(seq, Ordering::Release);
                            slot.state.store(SLOT_READY, Ordering::Release);
                            return;
                        }
                    } else {
                        return;
                    }
                }
                SLOT_WRITING => {
                    std::hint::spin_loop();
                }
                _ => unreachable!(),
            }
        }
    }
}

impl Source for JitterBufferConsumer {
    type Output = AudioFrame;

    fn pull(&mut self) -> Option<AudioFrame> {
        let read_seq = self.inner.read_seq.load(Ordering::Acquire);
        self.cached_read_seq = read_seq;

        let slot_idx = self.inner.slot_index(read_seq);
        let slot = &self.inner.slots[slot_idx];

        let state = slot.state.load(Ordering::Acquire);
        if state != SLOT_READY {
            return None;
        }

        let stored = slot.stored_seq.load(Ordering::Acquire);
        if stored != read_seq {
            return None;
        }

        let frame = unsafe { (*slot.data.get()).assume_init_read() };
        slot.state.store(SLOT_EMPTY, Ordering::Release);
        self.inner.read_seq.store(read_seq + 1, Ordering::Release);
        self.cached_read_seq = read_seq + 1;

        Some(frame)
    }
}

impl JitterBufferConsumer {
    pub fn skip(&mut self) {
        let read_seq = self.inner.read_seq.load(Ordering::Acquire);
        self.inner.read_seq.store(read_seq + 1, Ordering::Release);
        self.cached_read_seq = read_seq + 1;
    }

    pub fn next_expected_seq(&self) -> u64 {
        self.cached_read_seq
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
        let (mut producer, mut consumer) = jitter_buffer(8);

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
        let (mut producer, mut consumer) = jitter_buffer(8);

        producer.push(make_frame(2));
        producer.push(make_frame(0));
        producer.push(make_frame(1));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }

    #[test]
    fn test_duplicate_ignored() {
        let (mut producer, mut consumer) = jitter_buffer(8);

        producer.push(make_frame(0));
        producer.push(make_frame(0));
        producer.push(make_frame(0));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_late_packet_ignored() {
        let (mut producer, mut consumer) = jitter_buffer(8);

        producer.push(make_frame(0));
        producer.push(make_frame(1));

        assert_eq!(consumer.pull().unwrap().sequence_number, 0);

        producer.push(make_frame(0));

        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert!(consumer.pull().is_none());
    }

    #[test]
    fn test_hole_returns_none() {
        let (mut producer, mut consumer) = jitter_buffer(8);

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
        let (mut producer, mut consumer) = jitter_buffer(8);

        producer.push(make_frame(1));
        producer.push(make_frame(2));

        assert!(consumer.pull().is_none());
        consumer.skip();
        assert_eq!(consumer.pull().unwrap().sequence_number, 1);
        assert_eq!(consumer.pull().unwrap().sequence_number, 2);
    }
}
