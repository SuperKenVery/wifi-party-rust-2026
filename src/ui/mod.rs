//! User interface components.
//!
//! This module provides the Dioxus-based UI for the application:
//!
//! - [`app`] - Main application entry point
//! - [`sidebar`] - Left sidebar with audio controls and status
//! - [`participants`] - Main content area showing connected hosts

mod app;
mod participants;
mod sidebar;

pub use app::App;
