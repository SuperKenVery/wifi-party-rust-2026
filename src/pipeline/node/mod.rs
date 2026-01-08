/// A node that accepts data in a push-based pipeline.
///
/// Input: the type of data this node receives
/// Output: the type of data this node pushes to the next node
/// Next: the next node in the pipeline
///
/// AudioPipeline also implements xxNode<..., Next=()> because it's self-contained and
/// you don't need to care about its inner.
pub trait PushNode<Next>: Send {
    type Input;
    type Output;

    fn push(&mut self, data: Self::Input, next: &mut Next);
}

/// A node that produces data in a pull-based pipeline.
///
/// Input: the type of data this node pulls from the next node
/// Output: the type of data this node produces
/// Next: the next node in the pipeline
///
/// AudioPipeline also implements xxNode<..., Next=()> because it's self-contained and
/// you don't need to care about its inner.
pub trait PullNode<Prev>: Send {
    type Input;
    type Output;

    fn pull(&mut self, next: &mut Prev) -> Option<Self::Output>;
}

pub mod inspect;
pub mod jitter_buffer;
pub mod mix_pull;
pub mod mixer;
pub mod network_push;
pub mod null_node;
pub mod sinewave;
pub mod tee;
