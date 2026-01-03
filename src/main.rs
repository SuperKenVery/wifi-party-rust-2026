mod audio;
mod network;
mod state;
mod ui;

use dioxus::prelude::*;
use std::sync::Arc;
use std::net::{UdpSocket, IpAddr};
use tracing::{info, error};

use audio::{capture::AudioCaptureHandler, playback::AudioPlaybackHandler, mixer::AudioMixer};
use network::{send::NetworkSender, receive::NetworkReceiver};
use state::{AppState, HostId};

/// Get the local IP address by creating a socket
/// This doesn't actually send any data, just queries the local routing table
fn get_local_ip() -> Result<HostId, String> {
    // Create a UDP socket and connect to a multicast address
    // This doesn't send any data, but tells us which interface would be used
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("Failed to create socket: {}", e))?;
    
    socket.connect("239.255.43.2:7667")
        .map_err(|e| format!("Failed to connect socket: {}", e))?;
    
    let local_addr = socket.local_addr()
        .map_err(|e| format!("Failed to get local address: {}", e))?;
    
    match local_addr.ip() {
        IpAddr::V4(ipv4) => Ok(HostId::from(ipv4.octets())),
        IpAddr::V6(_) => Err("IPv6 not supported".to_string()),
    }
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();

    if let Err(e) = run() {
        error!("Application error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    info!("Starting Wi-Fi Party KTV...");

    // Create application state
    let state = Arc::new(AppState::new());

    // Set local host ID from local IP address
    if let Ok(local_ip) = get_local_ip() {
        info!("Local IP address: {}", local_ip.to_string());
        *state.local_host_id.lock().unwrap() = Some(local_ip);
    } else {
        error!("Failed to determine local IP address");
    }

    // Create SPSC queues for audio pipeline
    // Capture -> Network Send (larger buffer for UDP transmission)
    let (send_producer, send_consumer) = rtrb::RingBuffer::<Vec<u8>>::new(500);
    
    // Mixer -> Playback (for mixed network audio)
    let (playback_producer, playback_consumer) = rtrb::RingBuffer::<Vec<i16>>::new(100);

    // Capture -> Loopback (for hearing own voice)
    let (loopback_producer, loopback_consumer) = rtrb::RingBuffer::<Vec<i16>>::new(100);

    // Create crossbeam channel for network receive -> mixer
    // Channel carries (HostId, AudioFrame) tuples since host ID is extracted from UDP source
    let (frame_sender, frame_receiver) = crossbeam_channel::unbounded();

    // Start network threads
    let state_clone = state.clone();
    std::thread::spawn(move || {
        if let Err(e) = NetworkSender::start(state_clone, send_consumer) {
            error!("Failed to start network sender: {}", e);
        }
    });

    let state_clone = state.clone();
    std::thread::spawn(move || {
        if let Err(e) = NetworkReceiver::start(state_clone, frame_sender) {
            error!("Failed to start network receiver: {}", e);
        }
    });

    // Start mixer thread
    let state_clone = state.clone();
    std::thread::spawn(move || {
        if let Err(e) = AudioMixer::start(state_clone, frame_receiver, playback_producer) {
            error!("Failed to start audio mixer: {}", e);
        }
    });

    // Start audio capture
    let state_clone = state.clone();
    std::thread::spawn(move || {
        match AudioCaptureHandler::start(state_clone, send_producer, loopback_producer) {
            Ok(_capture) => {
                info!("Audio capture started, keeping alive...");
                // Keep the capture alive
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(e) => {
                error!("Failed to start audio capture: {}", e);
            }
        }
    });

    // Start audio playback (mixes network audio and loopback)
    std::thread::spawn(move || {
        match AudioPlaybackHandler::start(playback_consumer, loopback_consumer) {
            Ok(_playback) => {
                info!("Audio playback started, keeping alive...");
                // Keep the playback alive
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(e) => {
                error!("Failed to start audio playback: {}", e);
            }
        }
    });

    // Give threads time to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    info!("All components started, launching UI...");

    // Launch Dioxus UI with window configuration
    dioxus::LaunchBuilder::desktop()
        .with_cfg(dioxus::desktop::Config::new()
            .with_window(dioxus::desktop::WindowBuilder::new()
                .with_title("Wi-Fi Party KTV")
                .with_always_on_top(false)))
        .launch(ui::App);

    Ok(())
}