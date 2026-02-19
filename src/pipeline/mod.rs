//! Data processing pipeline framework.
//!
//! This module provides a dynamic pipeline architecture for audio processing:
//!
//! # Core Traits
//!
//! - [`Node`] - A processing unit that transforms input to output
//! - [`Pushable`]/[`Pullable`] - Object-safe traits for push/pull operations
//! - [`DynNode`] - Processing unit implementing both Pushable and Pullable
//! - [`DynSource`]/[`DynSink`] - Active data producers/consumers
//! - [`GraphNode`] - Wrapper to make any Node implement dynamic traits
//!
//! # Pipeline Construction
//!
//! Use macros for declarative pipeline construction:
//!
//! - [`push_chain!`] - Build push-based pipelines (e.g., mic -> encoder -> network)
//! - [`pull_chain!`] - Build pull-based pipelines (e.g., mixer -> switch -> speaker)
//!
//! # Example
//!
//! ```ignore
//! // Push chain: mic audio flows through processing to network
//! let mic_pipeline = push_chain![
//!     LevelMeter::new(level.clone()),
//!     Gain::new(volume.clone()),
//!     => network_sink.clone()
//! ];
//!
//! // Pull chain: speaker pulls from mixer through a switch
//! let output = pull_chain![
//!     mixer.clone() =>,
//!     Switch::new(enabled.clone())
//! ];
//! ```

pub mod dyn_traits;
pub mod graph_node;
pub mod traits;

pub use dyn_traits::{wrap_node, DynNode, DynSink, DynSource, Pullable, Pushable};
pub use graph_node::{GraphNode, OutputId};
pub use traits::Node;
