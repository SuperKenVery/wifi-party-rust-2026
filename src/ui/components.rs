use dioxus::prelude::*;
use crate::state::{AppState, ConnectionStatus, HostInfo};
use std::sync::Arc;

#[component]
pub fn App() -> Element {
    // Get state from context or create it
    let state_arc = use_context_provider(|| Arc::new(AppState::new()));
    
    // Create signals for reactive UI
    let mut connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let mut active_hosts = use_signal(|| Vec::<HostInfo>::new());
    let mut mic_muted = use_signal(|| false);
    let mut mic_volume = use_signal(|| 1.0f32);
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

                // Update loopback status
                loopback_enabled.set(state.loopback_enabled.load(std::sync::atomic::Ordering::Relaxed));

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
    loopback_enabled: bool,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let state_clone = state_arc.clone();
    let on_mute_toggle = move |_| {
        let current = state_clone.mic_muted.load(std::sync::atomic::Ordering::Relaxed);
        state_clone.mic_muted.store(!current, std::sync::atomic::Ordering::Relaxed);
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
        let current = state_clone3.loopback_enabled.load(std::sync::atomic::Ordering::Relaxed);
        state_clone3.loopback_enabled.store(!current, std::sync::atomic::Ordering::Relaxed);
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
        }
    }
}

#[component]
fn ParticipantsSection(
    hosts: Vec<HostInfo>,
) -> Element {
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
fn HostCard(
    host: HostInfo,
) -> Element {
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
