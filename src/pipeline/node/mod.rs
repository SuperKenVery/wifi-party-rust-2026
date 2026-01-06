use crate::audio::frame::AudioBuffer;

/// A node that accepts audio frames in a push-based pipeline.
///
/// When Next=(), it means that this node is terminal node. 
/// 
/// AudioPipeline also implements xxNode<..., Next=()> because it's self-contained and 
/// you don't need to care about its inner.
pub trait PushNode<const CHANNELS: usize, const SAMPLE_RATE: u32, Next = ()>: Send {
    fn push(&mut self, frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, next: &mut Next);
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next> PushNode<CHANNELS, SAMPLE_RATE, Next>
    for ()
{
    fn push(&mut self, _frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, _next: &mut Next) {}
}

/// A node that produces audio frames in a pull-based pipeline.
///
/// When Next=(), it means that this node is terminal node. 
/// 
/// AudioPipeline also implements xxNode<..., Next=()> because it's self-contained and 
/// you don't need to care about its inner.
pub trait PullNode<const CHANNELS: usize, const SAMPLE_RATE: u32, Next = ()>: Send {
    fn pull(&mut self, next: &mut Next) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>>;
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next> PullNode<CHANNELS, SAMPLE_RATE, Next>
    for ()
{
    fn pull(&mut self, _next: &mut Next) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> {
        None
    }
}

pub mod jitter_buffer;
pub mod mixer;
pub mod mix_pull;
pub mod network_push;
pub mod tee;
