use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::pipeline::Pullable;

/// A compressed audio packet.
pub struct CompressedPacket {
    pub dur: u32,
    pub data: Vec<u8>,
}

/// Thread-safe queue of compressed audio packets.
///
/// Packets are pushed in by the network receive path and pulled by the decoder.
/// Uses interior mutability for thread safety.
pub struct CompressedPacketQueue {
    queue: Mutex<VecDeque<CompressedPacket>>,
    packets_pushed: AtomicU64,
    highest_seq: AtomicU64,
}

impl CompressedPacketQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            packets_pushed: AtomicU64::new(0),
            highest_seq: AtomicU64::new(0),
        }
    }

    /// Enqueue a compressed packet and update counters.
    pub fn push_packet(&self, seq: u64, dur: u32, data: Vec<u8>) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(CompressedPacket { dur, data });
        self.packets_pushed.fetch_add(1, Ordering::Relaxed);
        self.highest_seq.fetch_max(seq, Ordering::Relaxed);
    }

    /// Total number of packets pushed since creation or last reset.
    pub fn packets_pushed(&self) -> u64 {
        self.packets_pushed.load(Ordering::Relaxed)
    }

    /// Highest sequence number seen.
    pub fn highest_seq(&self) -> u64 {
        self.highest_seq.load(Ordering::Relaxed)
    }

    /// Clear the queue (for seek).
    pub fn reset(&self) {
        let mut queue = self.queue.lock().unwrap();
        queue.clear();
    }
}

impl Pullable<CompressedPacket> for CompressedPacketQueue {
    /// Pop the front packet. `len` is ignored since packets have variable size.
    fn pull(&self, _len: usize) -> Option<CompressedPacket> {
        let mut queue = self.queue.lock().unwrap();
        queue.pop_front()
    }
}
