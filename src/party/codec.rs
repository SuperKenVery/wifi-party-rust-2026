//! Audio frame encoding and decoding.
//!
//! Converts between `AudioBuffer` (raw samples) and `AudioFrame` (with sequence number).

use std::sync::atomic::{AtomicU64, Ordering};

use crate::audio::AudioSample;
use crate::audio::frame::{AudioBuffer, AudioFrame};
use crate::pipeline::Node;

pub struct FramePacker<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sequence_number: AtomicU64,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    FramePacker<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            sequence_number: AtomicU64::new(0),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for FramePacker<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for FramePacker<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let seq = self.sequence_number.fetch_add(1, Ordering::Relaxed) + 1;
        AudioFrame::new(seq, input.into_inner()).ok()
    }
}

pub struct FrameUnpacker<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    FrameUnpacker<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for FrameUnpacker<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for FrameUnpacker<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        AudioBuffer::new(input.samples.into_inner()).ok()
    }
}
