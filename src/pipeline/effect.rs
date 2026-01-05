use crate::pipeline::frame::PipelineFrame;

/// Pure audio processing effect.
/// Completely unaware of the pipeline architecture.
/// Only transforms audio data.
pub trait AudioEffect {
    fn process(&self, frame: &mut PipelineFrame);
}
