use crate::audio::{
    capture::AudioCaptureHandler, mixer::AudioMixer, playback::AudioPlaybackHandler,
};
use crate::network::{receive::NetworkReceiver, send::NetworkSender};
use crate::state::{AppState, ConnectionStatus, HostId, HostInfo};
use anyhow::{Context, Result};
use dioxus::prelude::*;
use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;
use tracing::{error, info};

/// Get the local IP address by creating a socket
/// This doesn't actually send any data, just queries the local routing table
pub fn get_local_ip() -> Result<HostId> {
    // Create a UDP socket and connect to a multicast address
    // This doesn't send any data, but tells us which interface would be used
    let socket = UdpSocket::bind("0.0.0.0:0").context("Failed to create socket")?;

    socket
        .connect("239.255.43.2:7667")
        .context("Failed to connect socket")?;

    let local_addr = socket.local_addr().context("Failed to get local address")?;

    match local_addr.ip() {
        IpAddr::V4(ipv4) => Ok(HostId::from(ipv4.octets())),
        IpAddr::V6(_) => anyhow::bail!("IPv6 not supported"),
    }
}

fn setup() -> Arc<AppState> {
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
    let (send_producer, send_consumer) = rtrb::RingBuffer::<Vec<u8>>::new(500);
    let (playback_producer, playback_consumer) = rtrb::RingBuffer::<Vec<i16>>::new(100);
    let (loopback_producer, loopback_consumer) = rtrb::RingBuffer::<Vec<i16>>::new(100);
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
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(e) => {
                error!("Failed to start audio capture: {}", e);
            }
        }
    });

    // Start audio playback
    std::thread::spawn(move || {
        match AudioPlaybackHandler::start(playback_consumer, loopback_consumer) {
            Ok(_playback) => {
                info!("Audio playback started, keeping alive...");
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(e) => {
                error!("Failed to start audio playback: {}", e);
            }
        }
    });

    state
}

pub fn App() -> Element {
    let state_arc = use_context_provider(|| setup());

    // Create signals for reactive UI
    let mut connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let mut active_hosts = use_signal(|| Vec::<HostInfo>::new());
    let mut mic_muted = use_signal(|| false);
    let mut mic_volume = use_signal(|| 1.0f32);
    let mut mic_audio_level = use_signal(|| 0.0f32);
    let mut loopback_enabled = use_signal(|| false);
    let mut local_host_id = use_signal(|| String::from("Unknown"));

    // Poll state periodically
    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            loop {
                // Update connection status
                if let Ok(status) = state.connection_status.lock() {
                    connection_status.set(*status);
                }

                // Update active hosts
                if let Ok(hosts) = state.active_hosts.lock() {
                    active_hosts.set(hosts.values().cloned().collect());
                }

                // Update mic muted status
                mic_muted.set(state.mic_muted.load(std::sync::atomic::Ordering::Relaxed));

                // Update mic volume
                if let Ok(vol) = state.mic_volume.lock() {
                    mic_volume.set(*vol);
                }

                // Update mic audio level
                if let Ok(level) = state.mic_audio_level.lock() {
                    let new_level = *level;
                    mic_audio_level.set(new_level);
                }

                // Update loopback status
                loopback_enabled.set(
                    state
                        .loopback_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                // Update local host ID
                if let Ok(id_opt) = state.local_host_id.lock() {
                    if let Some(id) = *id_opt {
                        local_host_id.set(id.to_string());
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    });

    rsx! {
        div {
            class: "container mx-auto p-4",

            // Header
            Header {
                connection_status: connection_status(),
                local_host_id: local_host_id(),
                participant_count: active_hosts().len(),
            }

            // Self Audio Section
            SelfAudioSection {
                mic_muted: mic_muted(),
                mic_volume: mic_volume(),
                mic_audio_level: mic_audio_level(),
                loopback_enabled: loopback_enabled(),
            }

            // Participants Section
            ParticipantsSection {
                hosts: active_hosts(),
            }

            // Statistics Panel
            StatisticsPanel {}
        }
    }
}

#[component]
fn Header(
    connection_status: ConnectionStatus,
    local_host_id: String,
    participant_count: usize,
) -> Element {
    let status_text = match connection_status {
        ConnectionStatus::Connected => "Connected",
        ConnectionStatus::Disconnected => "Disconnected",
    };

    let status_color = match connection_status {
        ConnectionStatus::Connected => "text-green-500",
        ConnectionStatus::Disconnected => "text-red-500",
    };

    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg mb-6",

            h1 {
                class: "text-3xl font-bold mb-4",
                "🎤 Wi-Fi Party KTV"
            }

            div {
                class: "flex items-center gap-4",

                div {
                    span { class: "text-gray-400", "Status: " }
                    span { class: status_color, "{status_text}" }
                }

                div {
                    span { class: "text-gray-400", "Host ID: " }
                    span { class: "font-mono", "{local_host_id}" }
                }

                div {
                    span { class: "text-gray-400", "Participants: " }
                    span { class: "font-bold", "{participant_count}" }
                }
            }
        }
    }
}

#[component]
fn SelfAudioSection(
    mic_muted: bool,
    mic_volume: f32,
    mic_audio_level: f32,
    loopback_enabled: bool,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let state_clone = state_arc.clone();
    let on_mute_toggle = move |_| {
        let current = state_clone
            .mic_muted
            .load(std::sync::atomic::Ordering::Relaxed);
        state_clone
            .mic_muted
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let state_clone2 = state_arc.clone();
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            let volume = value_str / 100.0;
            if let Ok(mut vol) = state_clone2.mic_volume.lock() {
                *vol = volume;
            }
        }
    };

    let state_clone3 = state_arc.clone();
    let on_loopback_toggle = move |_| {
        let current = state_clone3
            .loopback_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        state_clone3
            .loopback_enabled
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let mute_button_class = if mic_muted {
        "px-6 py-3 bg-red-600 hover:bg-red-700 text-white rounded-lg font-bold"
    } else {
        "px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-lg font-bold"
    };

    let mute_button_text = if mic_muted { "Unmute" } else { "Mute" };

    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg mb-6",

            h2 {
                class: "text-2xl font-bold mb-4",
                "Your Audio"
            }

            div {
                class: "flex items-center gap-6",

                button {
                    class: mute_button_class,
                    onclick: on_mute_toggle,
                    "{mute_button_text} Microphone"
                }

                button {
                    class: if loopback_enabled {
                        "px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-bold"
                    } else {
                        "px-6 py-3 bg-gray-600 hover:bg-gray-700 text-white rounded-lg font-bold"
                    },
                    onclick: on_loopback_toggle,
                    if loopback_enabled { "🎧 Loopback: ON" } else { "🎧 Loopback: OFF" }
                }

                div {
                    class: "flex-1",
                    label {
                        class: "block text-sm mb-2",
                        "Microphone Volume: {(mic_volume * 100.0) as i32}%"
                    }
                    input {
                        r#type: "range",
                        min: 0,
                        max: 200,
                        value: (mic_volume * 100.0) as i32,
                        class: "w-full",
                        oninput: on_volume_change,
                    }
                }
            }

            div {
                class: "mt-4",
                label {
                    class: "block text-sm mb-2",
                    "🎤 Microphone Level: {(mic_audio_level * 100.0) as i32}%"
                }
                div {
                    class: "relative w-full h-6 bg-gray-700 rounded-lg overflow-hidden border border-gray-600",
                    div {
                        class: "absolute h-full bg-gradient-to-r from-green-500 via-yellow-500 to-red-500 transition-all duration-100",
                        style: "width: {(mic_audio_level * 100.0).min(100.0)}%",
                    }
                }
            }
        }
    }
}

#[component]
fn ParticipantsSection(hosts: Vec<HostInfo>) -> Element {
    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg mb-6",

            h2 {
                class: "text-2xl font-bold mb-4",
                "Participants ({hosts.len()})"
            }

            if hosts.is_empty() {
                div {
                    class: "text-gray-400 text-center py-8",
                    "No other participants connected"
                }
            } else {
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    for host in hosts {
                        HostCard {
                            host: host.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HostCard(host: HostInfo) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let host_id = host.id;
    let state_clone = state_arc.clone();

    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            let volume = value_str / 100.0;
            if let Ok(mut hosts) = state_clone.active_hosts.lock() {
                if let Some(host_info) = hosts.get_mut(&host_id) {
                    host_info.volume = volume;
                }
            }
        }
    };

    rsx! {
        div {
            class: "bg-gray-700 p-4 rounded-lg",

            div {
                class: "flex items-center justify-between mb-2",

                div {
                    class: "font-mono text-sm",
                    "{host.id.to_string()}"
                }

                div {
                    class: "text-xs text-gray-400",
                    "Loss: {(host.packet_loss * 100.0) as i32}%"
                }
            }

            div {
                label {
                    class: "block text-sm mb-1",
                    "Volume: {(host.volume * 100.0) as i32}%"
                }
                input {
                    r#type: "range",
                    min: 0,
                    max: 200,
                    value: (host.volume * 100.0) as i32,
                    class: "w-full",
                    oninput: on_volume_change,
                }
            }
        }
    }
}

#[component]
fn StatisticsPanel() -> Element {
    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg",

            h2 {
                class: "text-2xl font-bold mb-4",
                "Statistics"
            }

            div {
                class: "grid grid-cols-3 gap-4",

                div {
                    class: "text-center",
                    div { class: "text-sm text-gray-400", "Latency" }
                    div { class: "text-2xl font-bold", "~20ms" }
                }

                div {
                    class: "text-center",
                    div { class: "text-sm text-gray-400", "Packet Loss" }
                    div { class: "text-2xl font-bold", "0%" }
                }

                div {
                    class: "text-center",
                    div { class: "text-sm text-gray-400", "Jitter" }
                    div { class: "text-2xl font-bold", "2ms" }
                }
            }
        }
    }
}
