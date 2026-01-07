use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::node::terminal::NullNode;
use crate::pipeline::node::{PullNode, PushNode};

/// An Audio Pipeline.
///
/// Like an axum router, it recursively contain the next pipeline node.
/// It's like a linked list in this sense.
///
/// # Example
/// ```rust
/// use crate::pipeline::node::
/// let pipeline = AudioPipeline::new((), )
/// ```
pub struct AudioPipeline<Node, Inner = NullNode> {
    pub inner: Inner,
    pub node: Node,
}

impl<Node, Inner> AudioPipeline<Node, Inner> {
    pub fn new(node: Node) -> Self
    where
        Inner: Default,
    {
        Self {
            inner: Inner::default(),
            node,
        }
    }

    pub fn connect<NewNode>(self, node: NewNode) -> AudioPipeline<NewNode, Self> {
        AudioPipeline { inner: self, node }
    }
}

// For PushNode: process with node (Effect), then push to inner (Next).
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Inner, Node, Sample>
    PushNode<CHANNELS, SAMPLE_RATE, Sample, ()> for AudioPipeline<Node, Inner>
where
    Sample: AudioSample,
    Node: PushNode<CHANNELS, SAMPLE_RATE, Sample, Inner>,
    Inner: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
{
    fn push(&mut self, frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, _next: &mut ()) {
        // We ignore _next because we push to our internal inner.
        self.node.push(frame, &mut self.inner);
    }
}

// For PullNode: pull from inner (Next), then process with node (Effect).
impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Inner, Node, Sample>
    PullNode<CHANNELS, SAMPLE_RATE, Sample, ()> for AudioPipeline<Node, Inner>
where
    Sample: AudioSample,
    Node: PullNode<CHANNELS, SAMPLE_RATE, Sample, Inner>,
    Inner: PullNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
{
    fn pull(&mut self, _next: &mut ()) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        // We ignore _next because we pull from our internal inner.
        self.node.pull(&mut self.inner)
    }
}
