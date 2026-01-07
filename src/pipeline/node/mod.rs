use crate::audio::{frame::AudioBuffer, AudioSample};

/// A node that accepts audio frames in a push-based pipeline.
///
/// AudioPipeline also implements xxNode<..., Next=()> because it's self-contained and
/// you don't need to care about its inner.
pub trait PushNode<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample, Next>: Send {
    fn push(&mut self, frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, next: &mut Next);
}

/// A node that produces audio frames in a pull-based pipeline.
///
/// When Next=(), it means that this node is terminal node.
///
/// AudioPipeline also implements xxNode<..., Next=()> because it's self-contained and
/// you don't need to care about its inner.
pub trait PullNode<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample, Next>: Send {
    fn pull(&mut self, next: &mut Next) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>;
}

pub mod inspect;
pub mod jitter_buffer;
pub mod mix_pull;
pub mod mixer;
pub mod network_push;
pub mod sinewave;
pub mod tee;
pub mod terminal;
