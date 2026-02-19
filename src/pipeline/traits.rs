//! Core pipeline traits.
//!
//! This module defines the fundamental abstraction for data processing:
//!
//! - [`Node`] - A processing unit that transforms input data to output data
//!
//! For push/pull operations, see [`Pushable`](super::Pushable) and
//! [`Pullable`](super::Pullable) in the `dyn_traits` module.

/// A processing node that transforms input to output.
///
/// Nodes are the building blocks of pipelines. They receive input data,
/// process it, and optionally produce output data.
///
/// Use [`GraphNode`](super::GraphNode) to wrap a `Node` and gain
/// [`Pushable`](super::Pushable) and [`Pullable`](super::Pullable) capabilities.
pub trait Node: Send + Sync {
    type Input;
    type Output;

    /// Process input data and optionally produce output.
    ///
    /// Returns `None` if the node is buffering data and not ready to emit output yet.
    fn process(&self, input: Self::Input) -> Option<Self::Output>;
}
