use crate::pipeline::node::{PullNode, PushNode};
use crate::pipeline::pipeline::{PullPipeline, PushPipeline};
use crate::state::AppState;
use anyhow::Result;
use std::sync::Arc;

/// The main Party struct that manages all audio pipelines
pub struct Party {
    state: Arc<AppState>,
    input_pipeline: Box<dyn PushNode + Send>,
    output_pipeline: Box<dyn PullNode + Send>,
}

impl Party {
    pub fn new(state: Arc<AppState>) -> Result<Self> {
        // Create pipelines - this will be built up with the chain API
        // For now, we'll create placeholder pipelines
        let input_pipeline = Box::new(PlaceholderPushNode);
        let output_pipeline = Box::new(PlaceholderPullNode);

        Ok(Self {
            state,
            input_pipeline,
            output_pipeline,
        })
    }

    pub fn push_frame(&mut self, frame: crate::pipeline::frame::PipelineFrame) {
        self.input_pipeline.push(frame);
    }

    pub fn pull_frame(&mut self) -> Option<crate::pipeline::frame::PipelineFrame> {
        self.output_pipeline.pull()
    }
}

// Placeholder nodes for now
struct PlaceholderPushNode;

impl PushNode for PlaceholderPushNode {
    fn push(&mut self, _frame: crate::pipeline::frame::PipelineFrame) {
        // Placeholder
    }
}

struct PlaceholderPullNode;

impl PullNode for PlaceholderPullNode {
    fn pull(&mut self) -> Option<crate::pipeline::frame::PipelineFrame> {
        None
    }
}
