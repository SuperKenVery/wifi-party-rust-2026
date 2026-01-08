use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::node::null_node::NullNode;
use crate::pipeline::node::{PullNode, PushNode};
use std::marker::PhantomData;

/// Pure audio processing effect.
pub trait AudioEffect<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>: Send + Sync
where
    Sample: AudioSample,
{
    fn process(&mut self, frame: &mut AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>);
}

pub struct EffectNode<Effect, Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
    Effect,
    PhantomData<Sample>,
);

// Blanket implementation for AudioEffect acting as a PushNode
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Effect, Sample, Next> PushNode<Next>
    for EffectNode<Effect, Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
    Effect: AudioEffect<Sample, CHANNELS, SAMPLE_RATE>,
    Next: PushNode<NullNode<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>, Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&mut self, mut frame: Self::Input, next: &mut Next) {
        self.0.process(&mut frame);
        let mut null = NullNode::new();
        next.push(frame, &mut null);
    }
}

// Blanket implementation for AudioEffect acting as a PullNode
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Effect, Sample, Next> PullNode<Next>
    for EffectNode<Effect, Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
    Effect: AudioEffect<Sample, CHANNELS, SAMPLE_RATE>,
    Next: PullNode<NullNode<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>, Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&mut self, next: &mut Next) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut null = NullNode::new();
        let mut frame = next.pull(&mut null)?;
        self.0.process(&mut frame);
        Some(frame)
    }
}

pub mod gain;
pub mod mute;
pub mod noise_gate;
