use crate::pipeline::frame::PipelineFrame;

/// A node that accepts audio frames in a push-based pipeline.
/// Data flows from source to sink (e.g., microphone -> effects -> network).
pub trait PushNode {
    fn push(&mut self, frame: PipelineFrame);
}

/// A node that produces audio frames in a pull-based pipeline.
/// Data flows from source to sink (e.g., network -> effects -> speaker).
pub trait PullNode {
    fn pull(&mut self) -> Option<PipelineFrame>;
}
