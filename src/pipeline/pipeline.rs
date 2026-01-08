use crate::pipeline::node::null_node::NullNode;
use crate::pipeline::node::{PullNode, PushNode};

/// A generic Pipeline that can flow any type through its nodes.
///
/// Like an axum router, it recursively contain the next pipeline node.
/// It's like a linked list in this sense.
///
/// # Example
/// ```rust
/// use crate::pipeline::pipeline::AudioPipeline;
/// let pipeline = AudioPipeline::new(some_node);
/// ```
pub struct AudioPipeline<Node, Inner> {
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
// Output is the intermediate type that Node produces and Inner consumes.
impl<Inner, Node> PushNode<NullNode<<Node as PushNode<Inner>>::Input, <Node as PushNode<Inner>>::Output>> for AudioPipeline<Node, Inner>
where
    Node: PushNode<Inner>,
    Inner: PushNode<NullNode<<Node as PushNode<Inner>>::Input, <Node as PushNode<Inner>>::Output>, Input = <Node as PushNode<Inner>>::Output>,
{
    type Input = <Node as PushNode<Inner>>::Input;
    type Output = <Node as PushNode<Inner>>::Output;

    fn push(&mut self, data: Self::Input, _next: &mut NullNode<Self::Input, Self::Output>) {
        // We ignore _next because we push to our internal inner.
        self.node.push(data, &mut self.inner);
    }
}

// For PullNode: pull from inner (Next), then process with node (Effect).
// InnerInput is the intermediate type that Inner produces and Node consumes.
impl<Inner, Node> PullNode<NullNode<<Node as PullNode<Inner>>::Input, <Node as PullNode<Inner>>::Output>> for AudioPipeline<Node, Inner>
where
    Node: PullNode<Inner>,
    Inner: PullNode<NullNode<<Node as PullNode<Inner>>::Input, <Node as PullNode<Inner>>::Output>, Output = <Node as PullNode<Inner>>::Input>,
{
    type Input = <Node as PullNode<Inner>>::Input;
    type Output = <Node as PullNode<Inner>>::Output;
    fn pull(&mut self, _next: &mut NullNode<Self::Input, Self::Output>) -> Option<Self::Output> {
        // We ignore _next because we pull from our internal inner.
        self.node.pull(&mut self.inner)
    }
}
