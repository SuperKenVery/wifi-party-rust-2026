use crate::{
    audio::frame::AudioBuffer,
    audio::AudioSample,
    pipeline::node::{PullNode, PushNode},
};

/// A node that does nothing. Useful as a terminal in pipeline.
pub struct NullNode;

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample, Next>
    PushNode<CHANNELS, SAMPLE_RATE, Sample, Next> for NullNode
where
    Sample: AudioSample,
{
    fn push(
        &mut self,
        _frame: crate::audio::frame::AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>,
        _next: &mut Next,
    ) {
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample, Next>
    PullNode<CHANNELS, SAMPLE_RATE, Sample, Next> for NullNode
where
    Sample: AudioSample,
{
    fn pull(&mut self, _next: &mut Next) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        None
    }
}
