use crate::pipeline::effect::AudioEffect;
use crate::pipeline::frame::PipelineFrame;
use crate::pipeline::node::{PullNode, PushNode};

/// Generic layer wrapper that combines an inner node with an effect.
/// This is the "glue" that allows chaining effects together.
pub struct AudioLayer<Inner, Effect> {
    pub inner: Inner,
    pub effect: Effect,
}

impl<Inner, Effect> AudioLayer<Inner, Effect> {
    pub fn new(inner: Inner, effect: Effect) -> Self {
        Self { inner, effect }
    }
}

/// For PushNode: process effect BEFORE passing to inner node.
/// This means effects execute in reverse order of chaining (outside-in).
impl<Inner, Effect> PushNode for AudioLayer<Inner, Effect>
where
    Inner: PushNode,
    Effect: AudioEffect,
{
    fn push(&mut self, mut frame: PipelineFrame) {
        self.effect.process(&mut frame);
        self.inner.push(frame);
    }
}

/// For PullNode: process effect AFTER getting data from inner node.
/// This means effects execute in order of chaining (inside-out).
impl<Inner, Effect> PullNode for AudioLayer<Inner, Effect>
where
    Inner: PullNode,
    Effect: AudioEffect,
{
    fn pull(&mut self) -> Option<PipelineFrame> {
        let mut frame = self.inner.pull()?;
        self.effect.process(&mut frame);
        Some(frame)
    }
}
