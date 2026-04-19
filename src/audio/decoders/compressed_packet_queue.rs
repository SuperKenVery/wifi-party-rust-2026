use std::sync::atomic::{AtomicU64, Ordering};

/// A compressed audio packet.
pub struct CompressedPacket {
    pub dur: u32,
    pub data: Vec<u8>,
}

/// Tracks packet progress counters for a synced music stream.
///
/// Packets are pushed directly into the pipeline; this struct only
/// records sequence numbers and counts for progress tracking.
pub struct PacketCounter {
    packets_pushed: AtomicU64,
    highest_seq: AtomicU64,
}

impl PacketCounter {
    pub fn new() -> Self {
        Self {
            packets_pushed: AtomicU64::new(0),
            highest_seq: AtomicU64::new(0),
        }
    }

    /// Record that a packet with the given sequence number was dispatched.
    pub fn record_packet(&self, seq: u64) {
        self.packets_pushed.fetch_add(1, Ordering::Relaxed);
        self.highest_seq.fetch_max(seq, Ordering::Relaxed);
    }

    /// Total number of packets recorded since creation.
    pub fn packets_pushed(&self) -> u64 {
        self.packets_pushed.load(Ordering::Relaxed)
    }

    /// Highest sequence number seen.
    pub fn highest_seq(&self) -> u64 {
        self.highest_seq.load(Ordering::Relaxed)
    }
}
