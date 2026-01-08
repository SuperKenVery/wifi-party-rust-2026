//! Pipeline composition types.
//!
//! This module provides [`PullPipeline`] and [`PushPipeline`] which are used
//! to compose nodes into processing chains.

use crate::pipeline::node::{Node, Sink, Source};

/// A pull-based pipeline that chains a source with a processing node.
///
/// Created by calling [`Source::pipe`] on a source. The resulting pipeline
/// also implements [`Source`], allowing further chaining.
///
/// Data flows from source through the node when [`Source::pull`] is called.
pub struct PullPipeline<S, N> {
    source: S,
    node: N,
}

impl<S, N> PullPipeline<S, N> {
    pub fn new(source: S, node: N) -> Self {
        Self { source, node }
    }
}

impl<S, N> Source for PullPipeline<S, N>
where
    S: Source,
    N: Node<Input = S::Output>,
{
    type Output = N::Output;

    fn pull(&mut self) -> Option<Self::Output> {
        let data = self.source.pull()?;
        self.node.process(data)
    }
}

/// A push-based pipeline that chains a processing node with a sink.
///
/// Created by calling [`Sink::pipe`] on a sink. The resulting pipeline
/// also implements [`Sink`], allowing further chaining.
///
/// Data flows through the node into the sink when [`Sink::push`] is called.
pub struct PushPipeline<N, S> {
    node: N,
    sink: S,
}

impl<N, S> PushPipeline<N, S> {
    pub fn new(node: N, sink: S) -> Self {
        Self { node, sink }
    }
}

impl<N, S> Sink for PushPipeline<N, S>
where
    N: Node,
    S: Sink<Input = N::Output>,
{
    type Input = N::Input;

    fn push(&mut self, input: Self::Input) {
        if let Some(output) = self.node.process(input) {
            self.sink.push(output);
        }
    }
}
