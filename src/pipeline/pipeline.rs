//! Pipeline composition types.
//!
//! This module provides [`PullPipeline`] and [`PushPipeline`] which are used
//! to compose nodes into processing chains.

use crate::pipeline::graph::{PipelineGraph, Inspectable};
use crate::pipeline::node::{Node, Sink, Source};

/// A pull-based pipeline that chains a source with a processing node.
///
/// Created by calling [`Source::give_data_to`] on a source. The resulting pipeline
/// also implements [`Source`], allowing further chaining.
///
/// Data flows from source through the node when [`Source::pull`] is called.
#[derive(Clone)]
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

    fn pull(&self) -> Option<Self::Output> {
        let data = self.source.pull()?;
        self.node.process(data)
    }
}

impl<S, N> Inspectable for PullPipeline<S, N>
where
    S: Source,
    N: Node<Input = S::Output>,
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let source_id = self.source.get_visual(graph);
        let node_id = self.node.get_visual(graph);
        graph.add_edge(source_id, node_id.clone(), None);
        node_id
    }
}

/// A push-based pipeline that chains a processing node with a sink.
///
/// Created by calling [`Sink::get_data_from`] on a sink. The resulting pipeline
/// also implements [`Sink`], allowing further chaining.
///
/// Data flows through the node into the sink when [`Sink::push`] is called.
#[derive(Clone)]
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

    fn push(&self, input: Self::Input) {
        if let Some(output) = self.node.process(input) {
            self.sink.push(output);
        }
    }
}

impl<N, S> Inspectable for PushPipeline<N, S>
where
    N: Node,
    S: Sink<Input = N::Output>,
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let node_id = self.node.get_visual(graph);
        let sink_id = self.sink.get_visual(graph);
        graph.add_edge(node_id.clone(), sink_id, None);
        node_id
    }
}
