//! Object-safe dynamic pipeline traits.
//!
//! These traits enable heterogeneous collections of pipeline components and
//! runtime graph modification, unlike the static [`Node`]/[`Source`]/[`Sink`]
//! traits which use associated types and are not dyn-compatible.
//!
//! # Relationship to Static Traits
//!
//! `Pullable<T>` and `Pushable<T>` are the dyn-compatible equivalents of
//! `Source` and `Sink`. Blanket impls automatically bridge them:
//! - Any `Source<Output=T>` implements `Pullable<T>`
//! - Any `Sink<Input=T>` implements `Pushable<T>`
//!
//! # Trait Hierarchy
//!
//! - [`Pushable<T>`] - Can receive pushed data (= dyn-safe `Sink`)
//! - [`Pullable<T>`] - Can return data when pulled (= dyn-safe `Source`)
//! - [`DynNode<I, O>`] - Processing unit that is both Pushable and Pullable
//! - [`DynSource<T>`] - Active data producer that pushes into Pushables
//! - [`DynSink<T>`] - Active data consumer that pulls from Pullables

/// Passive receiver - can receive pushed data.
///
/// This is the input interface for nodes in a push-based data flow.
/// When data is pushed, the implementation decides what to do with it:
/// - Process and forward to outputs (default for [`GraphNode`])
/// - Store in a buffer (e.g., [`JitterBuffer`])
pub trait Pushable<T>: Send + Sync {
    fn push(&self, input: T);
}

/// Passive producer - can return data when pulled.
///
/// This is the output interface for nodes in a pull-based data flow.
/// When pulled, the implementation decides how to produce data:
/// - Pull from upstream, process, return (default for [`GraphNode`])
/// - Read from internal buffer (e.g., [`JitterBuffer`])
pub trait Pullable<T>: Send + Sync {
    fn pull(&self, len: usize) -> Option<T>;
}

/// Processing node - transforms input to output.
///
/// A `DynNode` is both `Pushable` (can receive input) and `Pullable` (can produce output).
/// The [`process`](DynNode::process) method defines the transformation logic.
///
/// Default behaviors (provided by [`GraphNode`] wrapper):
/// - Push: process input and forward output to connected destinations
/// - Pull: pull from connected input source, process, return output
///
/// Custom implementations (e.g., [`JitterBuffer`]) can override push/pull behavior
/// by implementing `Pushable` and `Pullable` directly without using `GraphNode`.
pub trait DynNode<I, O>: Pushable<I> + Pullable<O> {
    fn process(&self, input: I) -> Option<O>;
}

/// Active data source - drives push-based data flow.
///
/// Sources actively push data into the graph. Examples:
/// - Microphone callback pushing captured audio
/// - Network receiver thread pushing received packets
pub trait DynSource<T>: Send + Sync {
    fn push_to(&self, sink: &dyn Pushable<T>);
}

/// Active data sink - drives pull-based data flow.
///
/// Sinks actively pull data from the graph. Examples:
/// - Speaker callback pulling audio for playback
/// - Network sender pulling packets to transmit
pub trait DynSink<T>: Send + Sync {
    fn pull_from(&self, source: &dyn Pullable<T>, len: usize);
}

use super::traits::{Sink, Source};

impl<T, S> Pullable<T> for S
where
    S: Source<Output = T>,
{
    fn pull(&self, len: usize) -> Option<T> {
        Source::pull(self, len)
    }
}

impl<T, S> Pushable<T> for S
where
    S: Sink<Input = T>,
{
    fn push(&self, input: T) {
        Sink::push(self, input)
    }
}
