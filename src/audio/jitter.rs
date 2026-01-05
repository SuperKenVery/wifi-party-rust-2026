use std::collections::VecDeque;

use crate::audio::AudioFrame;

pub struct HostJitterBuffer<const DEPTH: usize = 8> {
    /// Store future packets that arrived early
    buffer: VecDeque<Option<AudioFrame>>,
    /// If no packet drop and no out-of-order, what's the next packet's sequence number?
    next_expected_seq: Option<u64>,
}

impl<const DEPTH: usize> std::fmt::Debug for HostJitterBuffer<DEPTH> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostJitterBuffer")
            .field("depth", &DEPTH)
            .field("next_expected_seq", &self.next_expected_seq)
            .field("buffer_len", &self.buffer.len())
            .finish()
    }
}

impl<const DEPTH: usize> HostJitterBuffer<DEPTH> {
    pub fn new(_sample_rate: u32, _channels: u8) -> Self {
        Self {
            buffer: VecDeque::with_capacity(DEPTH),
            next_expected_seq: None,
        }
    }

    pub fn push(&mut self, frame: AudioFrame) {
        let seq = frame.sequence_number;

        let next_seq = match self.next_expected_seq {
            None => {
                self.next_expected_seq = Some(seq);
                // self.buffer.push_back(Some(frame));
                // return Ok(());
                seq
            }
            Some(n) => n,
        };

        // Duplicate packets
        if seq < next_seq {
            return;
        }

        let offset = (seq - next_seq) as usize;

        // Too far away in future, we should skip current data and keep up with it
        if offset >= DEPTH {
            self.buffer.clear();
            self.next_expected_seq = Some(seq);
            self.buffer.push_back(Some(frame));
            return;
        }

        while self.buffer.len() <= offset {
            self.buffer.push_back(None);
        }

        self.buffer[offset] = Some(frame);
    }

    pub fn pop(&mut self) -> Option<Vec<i16>> {
        if self.buffer.is_empty() {
            return None;
        }

        if let Some(frame_opt) = self.buffer.pop_front() {
            self.next_expected_seq = self.next_expected_seq.map(|n| n + 1);
            frame_opt.map(|f| f.samples.data().to_vec())
        } else {
            None
        }
    }
}
