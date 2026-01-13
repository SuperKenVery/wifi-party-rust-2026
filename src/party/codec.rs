//! Audio frame encoding and decoding.
//!
//! Converts between `AudioBuffer` (raw samples) and `AudioFrame` (with sequence number).

use std::sync::atomic::{AtomicU64, Ordering};

use crate::audio::AudioSample;
use crate::audio::frame::{AudioBuffer, AudioFrame};
use crate::pipeline::graph::{PipelineGraph, Inspectable};
use crate::pipeline::Node;

#[derive(Clone)]
pub struct FramePacker<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sequence_number: std::sync::Arc<AtomicU64>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    FramePacker<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            sequence_number: std::sync::Arc::new(AtomicU64::new(0)),
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

impl<Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Inspectable
    for FramePacker<Sample, CHANNELS, SAMPLE_RATE>
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        let svg = format!(
            r#"<div class="w-full h-full bg-blue-900 border border-blue-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-blue-200">Packer</div>
            </div>"#
        );
        graph.add_node(id.clone(), svg);
        id
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

#[derive(Clone)]
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

impl<Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Inspectable
    for FrameUnpacker<Sample, CHANNELS, SAMPLE_RATE>
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        let svg = format!(
            r#"<div class="w-full h-full bg-blue-900 border border-blue-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-blue-200">Unpacker</div>
            </div>"#
        );
        graph.add_node(id.clone(), svg);
        id
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
