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
use tracing::{error, info, Level};

use crate::ui::get_local_ip;

fn main() {
    dioxus::logger::init(Level::DEBUG).expect("failed to init logger");

    if let Err(e) = run() {
        error!("Application error: {:?}", e);
        std::process::exit(1);
    }
}

fn setup_state() -> Result<Arc<AppState>> {
    let state = Arc::new(AppState::new());

    if let Ok(local_ip) = get_local_ip() {
        info!("Local IP address: {}", local_ip.to_string());
        *state.local_host_id.lock().unwrap() = Some(local_ip);
    } else {
        error!("Failed to determine local IP address");
    }

    Ok(state)
}

fn run() -> Result<()> {
    info!("Starting Wi-Fi Party KTV...");

    let state = setup_state().context("Failed to setup application state")?;

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
