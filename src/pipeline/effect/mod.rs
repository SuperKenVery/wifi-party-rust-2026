use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::node::{PushNode, PullNode};

/// Pure audio processing effect.
pub trait AudioEffect<T, const CHANNELS: usize, const SAMPLE_RATE: u32>: Send + Sync
where
    T: AudioSample,
{
    fn process(&mut self, frame: &mut AudioBuffer<T, CHANNELS, SAMPLE_RATE>);
}

// Blanket implementation for AudioEffect acting as a PushNode
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, E, Next> PushNode<CHANNELS, SAMPLE_RATE, Next>
    for E
where
    E: AudioEffect<f32, CHANNELS, SAMPLE_RATE>,
    Next: PushNode<CHANNELS, SAMPLE_RATE, ()>,
{
    fn push(&mut self, mut frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, next: &mut Next) {
        self.process(&mut frame);
        next.push(frame, &mut ());
    }
}

// Blanket implementation for AudioEffect acting as a PullNode
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, E, Next> PullNode<CHANNELS, SAMPLE_RATE, Next>
    for E
where
    E: AudioEffect<f32, CHANNELS, SAMPLE_RATE>,
    Next: PullNode<CHANNELS, SAMPLE_RATE, ()>,
{
    fn pull(&mut self, next: &mut Next) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> {
        let mut frame = next.pull(&mut ())?;
        self.process(&mut frame);
        Some(frame)
    }
}

pub mod gain;
pub mod mute;
pub mod noise_gate;
