//! Data processing pipeline framework.
//!
//! This module provides a generic, statically-dispatched pipeline architecture
//! for processing data streams. Pipelines are built by composing nodes using
//! method chaining.
//!
//! # Core Traits
//!
//! - [`Node`] - A processing unit that transforms input to output
//! - [`Source`] - A data producer (pull-based)
//! - [`Sink`] - A data consumer (push-based)
//!
//! # Pipeline Composition
//!
//! ```text
//! Pull Pipeline (e.g., Network -> Speaker):
//!     source.give_data_to(node_a).give_data_to(node_b)
//!     Data flow: source -> node_a -> node_b -> consumer
//!
//! Push Pipeline (e.g., Microphone -> Network):
//!     sink.get_data_from(node_b).get_data_from(node_a)
//!     Data flow: producer -> node_a -> node_b -> sink
//! ```

pub mod chain;
pub mod traits;

pub use chain::{PullPipeline, PushPipeline};
pub use traits::{Node, Sink, Source};
