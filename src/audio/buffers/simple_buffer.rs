//! A simple thread-safe FIFO buffer for audio samples.
//!
//! Stores individual samples and returns variable-length AudioBuffers on pull.

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Sink, Source};
use std::collections::VecDeque;
use std::sync::Mutex;

/// A sample-based FIFO buffer that accepts AudioBuffers and returns variable-length AudioBuffers.
///
/// When pushed, incoming AudioBuffer samples are appended to the internal queue.
/// When pulled, exactly `len` samples are returned (or fewer if the buffer doesn't have enough).
#[derive(Clone)]
pub struct SimpleBuffer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    queue: std::sync::Arc<Mutex<VecDeque<Sample>>>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            queue: std::sync::Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>) {
        let mut queue = self.queue.lock().unwrap();
        for sample in input.into_inner() {
            queue.push_back(sample);
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return None;
        }

        let actual_len = len.min(queue.len());
        let samples: Vec<Sample> = queue.drain(..actual_len).collect();
        AudioBuffer::new(samples).ok()
    }
}
