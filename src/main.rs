mod audio;
mod network;
mod party;
mod pipeline;
mod state;
mod ui;

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use state::AppState;
use std::sync::{Arc, Mutex};
use tracing::{error, info, Level};

use crate::audio::frame::AudioBuffer;
use crate::network::{receive::NetworkReceiver, send::NetworkSender};
use crate::ui::get_local_ip;

fn main() {
    dioxus::logger::init(Level::DEBUG).expect("failed to init logger");

    if let Err(e) = run() {
        error!("Application error: {:?}", e);
        std::process::exit(1);
    }
}

fn setup() -> Result<Arc<AppState>> {
    // --- 1. Define audio format ---
    // We must choose a single format for the entire pipeline at compile time.
    const CHANNELS: usize = 2;
    const SAMPLE_RATE: u32 = 48000;

    // --- 2. Create application state ---
    let state = Arc::new(AppState::new());

    // --- 3. Set local host ID ---
    if let Ok(local_ip) = get_local_ip() {
        info!("Local IP address: {}", local_ip.to_string());
        *state.local_host_id.lock().unwrap() = Some(local_ip);
    } else {
        error!("Failed to determine local IP address");
    }

    // --- 4. Create network queue and start sender thread ---
    let (send_producer, send_consumer) = rtrb::RingBuffer::<Vec<u8>>::new(500);
    let state_clone = state.clone();
    std::thread::spawn(move || {
        if let Err(e) = NetworkSender::start(state_clone, send_consumer) {
            error!("Failed to start network sender: {}", e);
        }
    });

    // --- 5. Create Party with statically-typed pipelines ---
    let party = Arc::new(Mutex::new(party::build_party::<CHANNELS, SAMPLE_RATE>(
        state.clone(),
        send_producer,
    )));

    // --- 6. Start network receiver thread ---
    let state_clone = state.clone();
    std::thread::spawn(move || {
        if let Err(e) = NetworkReceiver::start(state_clone) {
            error!("Failed to start network receiver: {}", e);
        }
    });

    // --- 7. Start CPAL audio streams ---
    let host = cpal::default_host();
    let input_device = host
        .default_input_device()
        .context("No input device available")?;
    // TODO: We should query supported configs and choose ours.
    // For now, we assume the default config matches our static choice.
    let input_config = input_device.default_input_config()?;
    if input_config.sample_rate().0 != SAMPLE_RATE || input_config.channels() as usize != CHANNELS {
        error!(
            "Default input device format {:?} does not match required format ({}ch @ {}Hz)",
            input_config, CHANNELS, SAMPLE_RATE
        );
        // In a real app, we might try to find a matching config or resample.
    }

    let party_clone_input = party.clone();
    let input_stream = input_device.build_input_stream(
        &input_config.config(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let frame = AudioBuffer::<f32, CHANNELS, SAMPLE_RATE>::new(data.to_vec()).unwrap();
            party_clone_input.lock().unwrap().push_frame(frame);
        },
        |err| error!("An error occurred on the input audio stream: {}", err),
        None,
    )?;
    input_stream.play()?;
    std::mem::forget(input_stream);

    let output_device = host
        .default_output_device()
        .context("No output device available")?;
    let output_config = output_device.default_output_config()?;

    let party_clone_output = party.clone();
    let output_stream = output_device.build_output_stream(
        &output_config.config(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if let Some(frame) = party_clone_output.lock().unwrap().pull_frame() {
                data.copy_from_slice(frame.data());
            } else {
                for sample in data {
                    *sample = 0.0;
                }
            }
        },
        |err| error!("An error occurred on the output audio stream: {}", err),
        None,
    )?;
    output_stream.play()?;
    std::mem::forget(output_stream);

    info!("Application setup complete. Audio pipelines are live.");

    Ok(state)
}

fn run() -> Result<()> {
    info!("Starting Wi-Fi Party KTV...");

    // Setup application state and background threads
    let state = setup().context("Failed to setup application")?;

    // Launch Dioxus UI with window configuration
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
