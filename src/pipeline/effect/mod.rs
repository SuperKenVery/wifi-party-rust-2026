use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
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
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Effect, Sample, Next>
    PushNode<CHANNELS, SAMPLE_RATE, Sample, Next>
    for EffectNode<Effect, Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
    Effect: AudioEffect<Sample, CHANNELS, SAMPLE_RATE>,
    Next: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
{
    fn push(&mut self, mut frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, next: &mut Next) {
        self.0.process(&mut frame);
        next.push(frame, &mut ());
    }
}

// Blanket implementation for AudioEffect acting as a PullNode
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Effect, Sample, Next>
    PullNode<CHANNELS, SAMPLE_RATE, Sample, Next>
    for EffectNode<Effect, Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
    Effect: AudioEffect<Sample, CHANNELS, SAMPLE_RATE>,
    Next: PullNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
{
    fn pull(&mut self, next: &mut Next) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut frame = next.pull(&mut ())?;
        self.0.process(&mut frame);
        Some(frame)
    }
}

pub mod gain;
pub mod mute;
pub mod noise_gate;
