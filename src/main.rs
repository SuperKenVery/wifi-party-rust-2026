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
mod music_provider;
mod party;
mod pipeline;
mod platform_support;
mod state;
mod ui;

use anyhow::{Context, Result};
use party::PartyConfig;
use state::AppState;
use tracing::{error, info};

#[cfg(any(
    feature = "desktop",
    all(feature = "mobile", any(target_os = "android", target_os = "ios"))
))]
const CUSTOM_HEAD: &str =
    r#"<meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover">"#;

fn main() {
    init_logging();

    if let Err(e) = run() {
        error!("Application error: {:?}", e);
        std::process::exit(1);
    }
}

#[cfg(target_os = "ios")]
fn init_logging() {
    oslog::OsLogger::new("com.ken.WifiPartyRust")
        .level_filter(log::LevelFilter::Debug)
        .init()
        .expect("failed to init oslog logger");
}

#[cfg(not(target_os = "ios"))]
fn init_logging() {
    dioxus::logger::init(tracing::Level::DEBUG).expect("failed to init logger");
}

fn run() -> Result<()> {
    info!("Starting Wi-Fi Party...");
    // Deloxide::new()
    //     .callback(|info| {
    //         println!("Deadlock detected! Cycle: {:?}", info.thread_cycle);
    //     })
    //     .start()
    //     .expect("Failed to initialize detector");

    let config = PartyConfig::default();
    let state = AppState::new(config).context("Failed to initialize application")?;

    info!("Application setup complete. Audio pipelines are live.");

    #[cfg(all(feature = "mobile", any(target_os = "android", target_os = "ios")))]
    #[allow(unused_mut)]
    let mut launcher = dioxus::LaunchBuilder::mobile().with_context(state);

    #[cfg(not(all(feature = "mobile", any(target_os = "android", target_os = "ios"))))]
    #[allow(unused_mut)]
    let mut launcher = dioxus::LaunchBuilder::new().with_context(state);

    #[cfg(all(
        feature = "desktop",
        not(any(target_os = "android", target_os = "ios"))
    ))]
    {
        launcher = launcher.with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("Wi-Fi Party KTV")
                        .with_always_on_top(false),
                )
                .with_custom_head(CUSTOM_HEAD.into()),
        );
    }

    #[cfg(all(feature = "mobile", any(target_os = "android", target_os = "ios")))]
    {
        launcher =
            launcher.with_cfg(dioxus::mobile::Config::new().with_custom_head(CUSTOM_HEAD.into()));
    }

    launcher.launch(ui::App);

    Ok(())
}
