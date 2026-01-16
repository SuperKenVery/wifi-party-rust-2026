//! Pipeline node traits and implementations.
//!
//! This module provides the core abstractions for building audio processing pipelines:
//!
//! - [`Node`] - A processing unit that transforms input data to output data
//! - [`Source`] - A data producer that can be pulled from
//! - [`Sink`] - A data consumer that can be pushed into
//!
//! # Architecture
//!
//! Pipelines are built by chaining nodes together using the `.pipe()` method.
//! Data flows through the pipeline either by pushing (for input pipelines) or
//! pulling (for output pipelines).
//!
//! ```text
//! Push Pipeline (e.g., Microphone -> Network):
//!     sink.pipe(node_b).pipe(node_a)
//!     Data flow: input -> node_a -> node_b -> sink
//!
//! Pull Pipeline (e.g., Network -> Speaker):
//!     source.pipe(node_a).pipe(node_b)
//!     Data flow: source -> node_a -> node_b -> output
//! ```

pub mod jitter_buffer;
pub mod simple_buffer;

pub use jitter_buffer::JitterBuffer;
pub use simple_buffer::SimpleBuffer;

use crate::pipeline::{PullPipeline, PushPipeline};

pub trait Node: Send + Sync {
    type Input;
    type Output;

    fn process(&self, input: Self::Input) -> Option<Self::Output>;
}

pub trait Source: Send + Sync + Sized {
    type Output;

    fn pull(&self, len: usize) -> Option<Self::Output>;

    fn give_data_to<N: Node<Input = Self::Output>>(self, node: N) -> PullPipeline<Self, N> {
        PullPipeline::new(self, node)
    }
}

pub trait Sink: Send + Sync + Sized {
    type Input;

    fn push(&self, input: Self::Input);

    fn get_data_from<N: Node<Output = Self::Input>>(self, node: N) -> PushPipeline<N, Self> {
        PushPipeline::new(node, self)
    }
}
