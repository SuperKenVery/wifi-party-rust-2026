use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::{PullNode, PushNode};

/// An Audio Pipeline.
///
/// Like an axum router, it recursively contain the next pipeline node.
/// It's like a linked list in this sense.
pub struct AudioPipeline<Inner, Node> {
    pub inner: Inner,
    pub node: Node,
}

impl<Inner, Node> AudioPipeline<Inner, Node> {
    pub fn new(inner: Inner, node: Node) -> Self {
        Self { inner, node }
    }

    pub fn connect<NewNode>(self, node: NewNode) -> AudioPipeline<Self, NewNode> {
        AudioPipeline::new(self, node)
    }
}

// For PushNode: process with node (Effect), then push to inner (Next).
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Inner, Node> PushNode<CHANNELS, SAMPLE_RATE, ()>
    for AudioPipeline<Inner, Node>
where
    Node: PushNode<CHANNELS, SAMPLE_RATE, Inner>,
    Inner: PushNode<CHANNELS, SAMPLE_RATE, ()>,
{
    fn push(&mut self, frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, _next: &mut ()) {
        // We ignore _next because we push to our internal inner.
        self.node.push(frame, &mut self.inner);
    }
}

// For PullNode: pull from inner (Next), then process with node (Effect).
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Inner, Node> PullNode<CHANNELS, SAMPLE_RATE, ()>
    for AudioPipeline<Inner, Node>
where
    Node: PullNode<CHANNELS, SAMPLE_RATE, Inner>,
    Inner: PullNode<CHANNELS, SAMPLE_RATE, ()>,
{
    fn pull(&mut self, _next: &mut ()) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> {
        // We ignore _next because we pull from our internal inner.
        self.node.pull(&mut self.inner)
    }
}
