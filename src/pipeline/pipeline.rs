use crate::pipeline::effect::AudioEffect;
use crate::pipeline::layer::AudioLayer;
use crate::pipeline::node::{PullNode, PushNode};

/// Builder for push-based pipelines (source -> sink).
/// Allows chaining effects with `.layer()` calls.
pub struct PushPipeline<T>(T);

impl<T: PushNode> PushPipeline<T> {
    pub fn new(root: T) -> Self {
        Self(root)
    }

    /// Add an effect layer. Effects execute in reverse order (outside-in).
    /// First layer added executes last, last layer added executes first.
    pub fn layer<E: AudioEffect>(self, effect: E) -> PushPipeline<AudioLayer<T, E>> {
        PushPipeline(AudioLayer {
            inner: self.0,
            effect,
        })
    }

    /// Consume the builder and return the final node.
    pub fn build(self) -> T {
        self.0
    }
}

/// Builder for pull-based pipelines (source -> sink).
/// Allows chaining effects with `.layer()` calls.
pub struct PullPipeline<T>(T);

impl<T: PullNode> PullPipeline<T> {
    pub fn new(root: T) -> Self {
        Self(root)
    }

    /// Add an effect layer. Effects execute in order (inside-out).
    /// First layer added executes first, last layer added executes last.
    pub fn layer<E: AudioEffect>(self, effect: E) -> PullPipeline<AudioLayer<T, E>> {
        PullPipeline(AudioLayer {
            inner: self.0,
            effect,
        })
    }

    /// Consume the builder and return the final node.
    pub fn build(self) -> T {
        self.0
    }
}
