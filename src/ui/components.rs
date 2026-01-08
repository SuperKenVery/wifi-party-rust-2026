use crate::state::{AppState, ConnectionStatus, HostInfo, StateUpdate};
use dioxus::prelude::*;
use std::sync::Arc;

#[allow(non_snake_case)]
pub fn App() -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    // Create signals for reactive UI
    let mut connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let mut active_hosts = use_signal(|| Vec::<HostInfo>::new());
    let mut mic_muted = use_signal(|| false);
    let mut mic_volume = use_signal(|| 1.0f32);
    let mut mic_audio_level = use_signal(|| 0.0f32);
    let mut loopback_enabled = use_signal(|| false);
    let mut local_host_id = use_signal(|| String::from("Unknown"));

    // Initialize state from current values
    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            // Initial state load
            if let Ok(status) = state.connection_status.lock() {
                connection_status.set(*status);
            }
            if let Ok(hosts) = state.active_hosts.lock() {
                active_hosts.set(hosts.values().cloned().collect());
            }
            mic_muted.set(state.mic_muted.load(std::sync::atomic::Ordering::Relaxed));
            if let Ok(vol) = state.mic_volume.lock() {
                mic_volume.set(*vol);
            }
            if let Ok(level) = state.mic_audio_level.lock() {
                mic_audio_level.set(*level);
            }
            loopback_enabled.set(
                state
                    .loopback_enabled
                    .load(std::sync::atomic::Ordering::Relaxed),
            );
            if let Ok(id_opt) = state.local_host_id.lock() {
                if let Some(id) = *id_opt {
                    local_host_id.set(id.to_string());
                }
            }
        });
    });

    // Listen to state updates via channel (reactive, minimal polling)
    // This uses try_recv with a sleep, which is much more efficient than constant polling
    // because state changes are sent immediately via the channel
    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            loop {
                // Check for updates (non-blocking)
                let mut has_update = false;
                loop {
                    let update = {
                        let rx_guard = state.state_update_rx.lock().unwrap();
                        rx_guard.try_recv().ok()
                    };
                    
                    if let Some(update) = update {
                        has_update = true;
                        match update {
                            StateUpdate::ConnectionStatusChanged(status) => {
                                connection_status.set(status);
                            }
                            StateUpdate::ActiveHostsChanged(hosts) => {
                                active_hosts.set(hosts);
                            }
                            StateUpdate::MicMutedChanged(muted) => {
                                mic_muted.set(muted);
                            }
                            StateUpdate::MicVolumeChanged(vol) => {
                                mic_volume.set(vol);
                            }
                            StateUpdate::MicAudioLevelChanged(level) => {
                                mic_audio_level.set(level);
                            }
                            StateUpdate::LoopbackEnabledChanged(enabled) => {
                                loopback_enabled.set(enabled);
                            }
                            StateUpdate::LocalHostIdChanged(id) => {
                                local_host_id.set(id);
                            }
                        }
                    } else {
                        break;
                    }
                }
                
                // Only sleep if we didn't process any updates (avoids busy-waiting)
                // If we processed updates, immediately check again for batched updates
                if !has_update {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                }
            }
        });
    });

    rsx! {
        div {
            class: "min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900",
            style: "background: linear-gradient(135deg, #0f172a 0%, #581c87 50%, #0f172a 100%);",
            
            div {
                class: "container mx-auto p-6 max-w-6xl",
                
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
}

#[allow(non_snake_case)]
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
            class: "bg-gradient-to-r from-slate-800/90 to-slate-700/90 backdrop-blur-sm text-white p-8 rounded-2xl mb-6 shadow-2xl border border-slate-600/50",
            style: "box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.3), 0 10px 10px -5px rgba(0, 0, 0, 0.2);",

            div {
                class: "flex items-center justify-between mb-6",
                h1 {
                    class: "text-4xl font-bold bg-gradient-to-r from-purple-400 to-pink-400 bg-clip-text text-transparent",
                    "🎤 Wi-Fi Party KTV"
                }
                div {
                    class: "flex items-center gap-2 px-4 py-2 rounded-full bg-slate-700/50 backdrop-blur-sm",
                    div {
                        class: "w-3 h-3 rounded-full",
                        style: if connection_status == ConnectionStatus::Connected {
                            "background: linear-gradient(135deg, #10b981, #34d399); box-shadow: 0 0 10px rgba(16, 185, 129, 0.5);"
                        } else {
                            "background: linear-gradient(135deg, #ef4444, #f87171); box-shadow: 0 0 10px rgba(239, 68, 68, 0.5);"
                        },
                    }
                    span { 
                        class: "text-sm font-semibold",
                        "{status_text}"
                    }
                }
            }

            div {
                class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                
                div {
                    class: "bg-slate-700/30 rounded-xl p-4 backdrop-blur-sm border border-slate-600/30",
                    div { class: "text-xs text-slate-400 mb-1 uppercase tracking-wide", "Host ID" }
                    div { class: "font-mono text-sm font-semibold text-purple-300", "{local_host_id}" }
                }

                div {
                    class: "bg-slate-700/30 rounded-xl p-4 backdrop-blur-sm border border-slate-600/30",
                    div { class: "text-xs text-slate-400 mb-1 uppercase tracking-wide", "Participants" }
                    div { class: "text-2xl font-bold text-pink-300", "{participant_count}" }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
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
        state_clone.set_mic_muted(!current);
    };

    let state_clone2 = state_arc.clone();
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            let volume = value_str / 100.0;
            state_clone2.set_mic_volume(volume);
        }
    };

    let state_clone3 = state_arc.clone();
    let on_loopback_toggle = move |_| {
        let current = state_clone3
            .loopback_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        state_clone3.set_loopback_enabled(!current);
    };

    let mute_button_class = if mic_muted {
        "px-8 py-4 bg-gradient-to-r from-red-600 to-red-500 hover:from-red-700 hover:to-red-600 text-white rounded-xl font-bold shadow-lg hover:shadow-xl transition-all duration-200 transform hover:scale-105 active:scale-95"
    } else {
        "px-8 py-4 bg-gradient-to-r from-emerald-600 to-emerald-500 hover:from-emerald-700 hover:to-emerald-600 text-white rounded-xl font-bold shadow-lg hover:shadow-xl transition-all duration-200 transform hover:scale-105 active:scale-95"
    };

    let mute_button_text = if mic_muted { "🔇 Unmute" } else { "🎤 Mute" };

    rsx! {
        div {
            class: "bg-gradient-to-r from-slate-800/90 to-slate-700/90 backdrop-blur-sm text-white p-8 rounded-2xl mb-6 shadow-2xl border border-slate-600/50",
            style: "box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.3), 0 10px 10px -5px rgba(0, 0, 0, 0.2);",

            h2 {
                class: "text-3xl font-bold mb-6 bg-gradient-to-r from-purple-300 to-pink-300 bg-clip-text text-transparent",
                "Your Audio"
            }

            div {
                class: "flex flex-col md:flex-row items-stretch md:items-center gap-4 mb-6",
                
                button {
                    class: mute_button_class,
                    onclick: on_mute_toggle,
                    "{mute_button_text}"
                }

                button {
                    class: if loopback_enabled {
                        "px-8 py-4 bg-gradient-to-r from-blue-600 to-blue-500 hover:from-blue-700 hover:to-blue-600 text-white rounded-xl font-bold shadow-lg hover:shadow-xl transition-all duration-200 transform hover:scale-105 active:scale-95"
                    } else {
                        "px-8 py-4 bg-gradient-to-r from-slate-600 to-slate-500 hover:from-slate-700 hover:to-slate-600 text-white rounded-xl font-bold shadow-lg hover:shadow-xl transition-all duration-200 transform hover:scale-105 active:scale-95"
                    },
                    onclick: on_loopback_toggle,
                    if loopback_enabled { "🎧 Loopback: ON" } else { "🎧 Loopback: OFF" }
                }

                div {
                    class: "flex-1 bg-slate-700/30 rounded-xl p-4 backdrop-blur-sm border border-slate-600/30",
                    label {
                        class: "block text-sm mb-3 font-semibold text-slate-300",
                        "Microphone Volume: {(mic_volume * 100.0) as i32}%"
                    }
                    input {
                        r#type: "range",
                        min: 0,
                        max: 200,
                        value: (mic_volume * 100.0) as i32,
                        class: "w-full h-2 bg-slate-600 rounded-lg appearance-none cursor-pointer accent-purple-500",
                        style: "background: linear-gradient(to right, #8b5cf6 0%, #8b5cf6 {(mic_volume * 50.0) as i32}%, #475569 {(mic_volume * 50.0) as i32}%, #475569 100%);",
                        oninput: on_volume_change,
                    }
                }
            }

            div {
                class: "mt-6",
                label {
                    class: "block text-sm mb-3 font-semibold text-slate-300",
                    "🎤 Microphone Level: {(mic_audio_level * 100.0) as i32}%"
                }
                div {
                    class: "relative w-full h-8 bg-slate-700/50 rounded-xl overflow-hidden border border-slate-600/50 shadow-inner",
                    div {
                        class: "absolute h-full bg-gradient-to-r from-emerald-500 via-yellow-400 to-red-500 transition-all duration-150 ease-out rounded-xl",
                        style: "width: {(mic_audio_level * 100.0).min(100.0)}%; box-shadow: 0 0 20px rgba(16, 185, 129, 0.3);",
                    }
                    div {
                        class: "absolute inset-0 flex items-center justify-center text-xs font-bold text-white drop-shadow-lg",
                        style: "text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);",
                        if mic_audio_level > 0.01 {
                            "{(mic_audio_level * 100.0) as i32}%"
                        } else {
                            ""
                        }
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn ParticipantsSection(hosts: Vec<HostInfo>) -> Element {
    rsx! {
        div {
            class: "bg-gradient-to-r from-slate-800/90 to-slate-700/90 backdrop-blur-sm text-white p-8 rounded-2xl mb-6 shadow-2xl border border-slate-600/50",
            style: "box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.3), 0 10px 10px -5px rgba(0, 0, 0, 0.2);",

            h2 {
                class: "text-3xl font-bold mb-6 bg-gradient-to-r from-purple-300 to-pink-300 bg-clip-text text-transparent",
                "Participants ({hosts.len()})"
            }

            if hosts.is_empty() {
                div {
                    class: "text-slate-400 text-center py-12 rounded-xl bg-slate-700/20 border border-slate-600/30 border-dashed",
                    div { class: "text-4xl mb-2", "👥" }
                    div { class: "text-lg font-medium", "No other participants connected" }
                    div { class: "text-sm mt-2", "Waiting for others to join..." }
                }
            } else {
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
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

#[allow(non_snake_case)]
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
            class: "bg-gradient-to-br from-slate-700/80 to-slate-600/80 backdrop-blur-sm p-5 rounded-xl border border-slate-500/50 shadow-lg hover:shadow-xl transition-all duration-200 hover:scale-[1.02]",
            
            div {
                class: "flex items-center justify-between mb-4",
                
                div {
                    class: "flex items-center gap-2",
                    div {
                        class: "w-2 h-2 rounded-full bg-emerald-400 animate-pulse",
                        style: "box-shadow: 0 0 8px rgba(16, 185, 129, 0.6);",
                    }
                    div {
                        class: "font-mono text-sm font-semibold text-purple-300",
                        "{host.id.to_string()}"
                    }
                }

                div {
                    class: "px-3 py-1 rounded-full bg-slate-600/50 text-xs font-medium",
                    class: if host.packet_loss > 0.1 {
                        "text-red-300"
                    } else if host.packet_loss > 0.05 {
                        "text-yellow-300"
                    } else {
                        "text-emerald-300"
                    },
                    "Loss: {(host.packet_loss * 100.0) as i32}%"
                }
            }

            div {
                label {
                    class: "block text-sm mb-3 font-semibold text-slate-300",
                    "Volume: {(host.volume * 100.0) as i32}%"
                }
                input {
                    r#type: "range",
                    min: 0,
                    max: 200,
                    value: (host.volume * 100.0) as i32,
                    class: "w-full h-2 bg-slate-600 rounded-lg appearance-none cursor-pointer accent-purple-500",
                    style: "background: linear-gradient(to right, #8b5cf6 0%, #8b5cf6 {(host.volume * 50.0) as i32}%, #475569 {(host.volume * 50.0) as i32}%, #475569 100%);",
                    oninput: on_volume_change,
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn StatisticsPanel() -> Element {
    rsx! {
        div {
            class: "bg-gradient-to-r from-slate-800/90 to-slate-700/90 backdrop-blur-sm text-white p-8 rounded-2xl shadow-2xl border border-slate-600/50",
            style: "box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.3), 0 10px 10px -5px rgba(0, 0, 0, 0.2);",

            h2 {
                class: "text-3xl font-bold mb-6 bg-gradient-to-r from-purple-300 to-pink-300 bg-clip-text text-transparent",
                "Statistics"
            }

            div {
                class: "grid grid-cols-1 md:grid-cols-3 gap-6",

                div {
                    class: "text-center bg-slate-700/30 rounded-xl p-6 backdrop-blur-sm border border-slate-600/30 hover:bg-slate-700/40 transition-all duration-200",
                    div { 
                        class: "text-sm text-slate-400 mb-2 uppercase tracking-wide font-semibold",
                        "Latency"
                    }
                    div { 
                        class: "text-3xl font-bold bg-gradient-to-r from-blue-400 to-cyan-400 bg-clip-text text-transparent",
                        "~20ms"
                    }
                }

                div {
                    class: "text-center bg-slate-700/30 rounded-xl p-6 backdrop-blur-sm border border-slate-600/30 hover:bg-slate-700/40 transition-all duration-200",
                    div { 
                        class: "text-sm text-slate-400 mb-2 uppercase tracking-wide font-semibold",
                        "Packet Loss"
                    }
                    div { 
                        class: "text-3xl font-bold bg-gradient-to-r from-emerald-400 to-green-400 bg-clip-text text-transparent",
                        "0%"
                    }
                }

                div {
                    class: "text-center bg-slate-700/30 rounded-xl p-6 backdrop-blur-sm border border-slate-600/30 hover:bg-slate-700/40 transition-all duration-200",
                    div { 
                        class: "text-sm text-slate-400 mb-2 uppercase tracking-wide font-semibold",
                        "Jitter"
                    }
                    div { 
                        class: "text-3xl font-bold bg-gradient-to-r from-purple-400 to-pink-400 bg-clip-text text-transparent",
                        "2ms"
                    }
                }
            }
        }
    }
}
