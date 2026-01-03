mod audio;
mod network;
mod state;
mod ui;

use dioxus::prelude::*;
use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;
use tracing::{error, info, Level};

use audio::{capture::AudioCaptureHandler, mixer::AudioMixer, playback::AudioPlaybackHandler};
use network::{receive::NetworkReceiver, send::NetworkSender};
use state::{AppState, HostId};

fn main() {
    dioxus::logger::init(Level::DEBUG).expect("failed to init logger");

    if let Err(e) = run() {
        error!("Application error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    info!("Starting Wi-Fi Party KTV...");

    // Launch Dioxus UI with window configuration
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("Wi-Fi Party KTV")
                    .with_always_on_top(false),
            ),
        )
        .launch(ui::App);

    Ok(())
}
