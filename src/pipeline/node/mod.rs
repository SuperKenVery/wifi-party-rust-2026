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
//!
//! # Example
//!
//! ```ignore
//! use crate::pipeline::node::{Node, Source, Sink, JitterBuffer};
//! use crate::pipeline::{PushPipeline, PullPipeline};
//!
//! // Create a buffer that bridges push and pull pipelines
//! let buffer = JitterBuffer::<AudioBuffer>::new();
//!
//! // Push pipeline: data is pushed into the buffer
//! let push_pipeline = buffer.clone()
//!     .pipe(MyEncoder::new())
//!     .pipe(NoiseGate::new());
//! push_pipeline.push(audio_data);
//!
//! // Pull pipeline: data is pulled from the buffer
//! let pull_pipeline = buffer.clone()
//!     .pipe(MyDecoder::new())
//!     .pipe(Gain::new(0.8));
//! let output = pull_pipeline.pull();
//! ```

pub mod jitter_buffer;

pub use jitter_buffer::JitterBuffer;

use crate::pipeline::{PullPipeline, PushPipeline};

/// A processing node that transforms input data into output data.
///
/// Nodes are the building blocks of pipelines. Each node takes an input,
/// processes it, and optionally produces an output. Returning `None`
/// indicates that the data should not propagate further down the pipeline.
///
/// # Example
///
/// ```ignore
/// struct Gain(f32);
///
/// impl Node for Gain {
///     type Input = AudioBuffer;
///     type Output = AudioBuffer;
///
///     fn process(&mut self, mut input: AudioBuffer) -> Option<AudioBuffer> {
///         for sample in input.iter_mut() {
///             *sample *= self.0;
///         }
///         Some(input)
///     }
/// }
/// ```
pub trait Node: Send {
    type Input;
    type Output;

    fn process(&mut self, input: Self::Input) -> Option<Self::Output>;
}

/// A data source that produces values on demand (pull-based).
///
/// Sources are the starting point of pull pipelines. They produce data
/// when `pull()` is called, returning `None` when no data is available.
///
/// Use `.pipe(node)` to chain a processing node after the source.
/// The resulting `PullPipeline` also implements `Source`.
///
/// # Example
///
/// ```ignore
/// let source = JitterBuffer::new();
/// let pipeline = source
///     .pipe(Decoder::new())
///     .pipe(Gain::new(0.8));
///
/// // Pull data through the pipeline
/// if let Some(audio) = pipeline.pull() {
///     // Use audio...
/// }
/// ```
pub trait Source: Send + Sized {
    type Output;

    fn pull(&mut self) -> Option<Self::Output>;

    /// Chain a processing node after this source.
    ///
    /// Data flows: `self.pull()` -> `node.process()` -> output
    fn pipe<N: Node<Input = Self::Output>>(self, node: N) -> PullPipeline<Self, N> {
        PullPipeline::new(self, node)
    }
}

/// A data sink that consumes values (push-based).
///
/// Sinks are the endpoint of push pipelines. They receive data
/// when `push()` is called.
///
/// Use `.pipe(node)` to chain a processing node before the sink.
/// The resulting `PushPipeline` also implements `Sink`.
///
/// # Example
///
/// ```ignore
/// let sink = JitterBuffer::new();
/// let pipeline = sink
///     .pipe(Encoder::new())
///     .pipe(NoiseGate::new());
///
/// // Push data through the pipeline
/// // Data flows: NoiseGate -> Encoder -> JitterBuffer
/// pipeline.push(audio);
/// ```
pub trait Sink: Send + Sized {
    type Input;

    fn push(&mut self, input: Self::Input);

    /// Chain a processing node before this sink.
    ///
    /// Data flows: input -> `node.process()` -> `self.push()`
    ///
    /// Note: When chaining multiple nodes, data flows in reverse order
    /// of the `.pipe()` calls:
    /// ```ignore
    /// sink.pipe(B).pipe(A)  // Data flows: input -> A -> B -> sink
    /// ```
    fn pipe<N: Node<Output = Self::Input>>(self, node: N) -> PushPipeline<N, Self> {
        PushPipeline::new(node, self)
    }
}
