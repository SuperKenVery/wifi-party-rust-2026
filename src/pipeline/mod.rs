//! Data processing pipeline framework.
//!
//! This module provides both static and dynamic pipeline architectures:
//!
//! # Static Pipeline (compile-time composition)
//!
//! - [`Node`] - A processing unit that transforms input to output
//! - [`Source`] - A data producer (pull-based)
//! - [`Sink`] - A data consumer (push-based)
//! - [`PullPipeline`]/[`PushPipeline`] - Composed chains via method chaining
//!
//! # Dynamic Pipeline (runtime composition)
//!
//! - [`Pushable`]/[`Pullable`] - Object-safe traits for push/pull operations
//! - [`DynNode`] - Processing unit implementing both Pushable and Pullable
//! - [`DynSource`]/[`DynSink`] - Active data producers/consumers
//! - [`GraphNode`] - Wrapper to make any Node implement dynamic traits
//!
//! # When to Use Each
//!
//! - **Static**: Fixed pipelines known at compile time (e.g., mic -> encoder -> network)
//! - **Dynamic**: Pipelines that change at runtime (e.g., per-host decode chains)

pub mod chain;
pub mod dyn_traits;
pub mod graph_node;
pub mod traits;

pub use chain::{PullPipeline, PushPipeline};
pub use dyn_traits::{DynNode, DynSink, DynSource, Pullable, Pushable};
pub use graph_node::{GraphNode, OutputId};
pub use traits::{Node, Sink, Source};
