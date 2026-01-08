use crate::pipeline::node::{PullNode, PushNode};

/// A node that does nothing. Useful as a terminal in pipeline.
#[derive(Clone, Copy)]
pub struct NullNode<Input, Output>(std::marker::PhantomData<(Input, Output)>);

impl<Input, Output> Default for NullNode<Input, Output> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<Input, Output> NullNode<Input, Output> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<Input, Output, Next> PushNode<Next> for NullNode<Input, Output>
where
    Input: Send,
    Output: Send,
{
    type Input = Input;
    type Output = Output;

    fn push(&mut self, _data: Input, _next: &mut Next) {
        // Discard the data
    }
}

impl<Input, Output, Next> PullNode<Next> for NullNode<Input, Output>
where
    Input: Send,
    Output: Send,
{
    type Input = Input;
    type Output = Output;

    fn pull(&mut self, _next: &mut Next) -> Option<Output> {
        None
    }
}
