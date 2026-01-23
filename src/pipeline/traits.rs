//! Core pipeline traits.
//!
//! This module defines the fundamental abstractions for building data processing pipelines:
//!
//! - [`Node`] - A processing unit that transforms input data to output data
//! - [`Source`] - A data producer that can be pulled from
//! - [`Sink`] - A data consumer that can be pushed into

use super::{PullPipeline, PushPipeline};

/// A processing node that transforms input to output.
///
/// Nodes are the building blocks of pipelines. They receive input data,
/// process it, and optionally produce output data.
pub trait Node: Send + Sync {
    type Input;
    type Output;

    /// Process input data and optionally produce output.
    ///
    /// Returns `None` if the node is buffering data and not ready to emit output yet.
    fn process(&self, input: Self::Input) -> Option<Self::Output>;
}

/// A data source that can be pulled from.
///
/// Sources produce data on demand when [`pull`](Source::pull) is called.
pub trait Source: Send + Sync {
    type Output;

    /// Pull data from the source.
    ///
    /// The `len` parameter is a hint for how much data is requested.
    /// Returns `None` if no data is available.
    fn pull(&self, len: usize) -> Option<Self::Output>;

    /// Chain this source with a processing node, creating a pull pipeline.
    ///
    /// Data flows: `self` -> `node` -> consumer
    fn give_data_to<N: Node<Input = Self::Output>>(self, node: N) -> PullPipeline<Self, N>
    where
        Self: Sized,
    {
        PullPipeline::new(self, node)
    }
}

/// A data sink that can be pushed into.
///
/// Sinks consume data when [`push`](Sink::push) is called.
pub trait Sink: Send + Sync {
    type Input;

    /// Push data into the sink.
    fn push(&self, input: Self::Input);

    /// Chain a processing node before this sink, creating a push pipeline.
    ///
    /// Data flows: producer -> `node` -> `self`
    fn get_data_from<N: Node<Output = Self::Input>>(self, node: N) -> PushPipeline<N, Self>
    where
        Self: Sized,
    {
        PushPipeline::new(node, self)
    }
}
