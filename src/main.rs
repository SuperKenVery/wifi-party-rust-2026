//! Wi-Fi Party - Local network audio sharing application.
//!
//! This application enables real-time audio sharing between devices on the same
//! local network using UDP multicast. Each participant can hear audio from all
//! other participants mixed together.
//!
//! # Crate Structure
//!
//! - [`audio`] - Audio data types ([`AudioBuffer`](audio::AudioBuffer), [`AudioFrame`](audio::AudioFrame))
//! - [`pipeline`] - Generic data processing pipeline framework
//! - [`io`] - Hardware I/O (microphone, speaker, network)
//! - [`party`] - Audio sharing orchestration and mixing
//! - [`state`] - Application state and configuration
//! - [`ui`] - User interface

mod audio;
mod io;
mod party;
mod pipeline;
mod state;
mod ui;

use anyhow::{Context, Result};
use party::Party;
use state::AppState;
use std::sync::Arc;
use tracing::{Level, error, info};

fn main() {
    dioxus::logger::init(Level::DEBUG).expect("failed to init logger");

    if let Err(e) = run() {
        error!("Application error: {:?}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    info!("Starting Wi-Fi Party KTV...");

    let state = Arc::new(AppState::new());

    let mut party = Party::<f32, 2, 48000>::new(state.clone());
    party.run().context("Failed to start Party")?;

    info!("Application setup complete. Audio pipelines are live.");

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("Wi-Fi Party KTV")
                    .with_always_on_top(false),
            ),
        )
        .with_context(state)
        .launch(ui::App);

    Ok(())
}
