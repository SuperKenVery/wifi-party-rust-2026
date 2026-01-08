//! Audio processing pipeline framework.
//!
//! This module provides a generic, statically-dispatched pipeline architecture
//! for processing audio (or any data type). Inspired by axum's router pattern,
//! pipelines are built by composing nodes using nested structs.
//!
//! See [`node`] module for the core traits and [`pipeline`] module for the
//! pipeline composition types.

pub mod effect;
pub mod node;
pub mod pipeline;

pub use node::{Node, Sink, Source};
pub use pipeline::{PullPipeline, PushPipeline};
