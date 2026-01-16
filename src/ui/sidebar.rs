//! Sidebar components for audio controls and status display.

use crate::state::{AppState, ConnectionStatus};
use dioxus::prelude::*;
use std::sync::Arc;

#[allow(non_snake_case)]
#[component]
pub fn Sidebar(
    connection_status: ConnectionStatus,
    mic_enabled: bool,
    mic_volume: f32,
    mic_audio_level: u32,
    loopback_enabled: bool,
) -> Element {
    let (status_text, status_color, status_bg) = match connection_status {
        ConnectionStatus::Connected => ("Online", "text-emerald-400", "bg-emerald-500"),
        ConnectionStatus::Disconnected => ("Offline", "text-rose-400", "bg-rose-500"),
    };

    rsx! {
        div {
            class: "w-80 flex-shrink-0 flex flex-col glass-strong border-r border-slate-800 z-20",

            div {
                class: "p-8 pb-4",
                div {
                    class: "flex items-center gap-3 mb-1",
                    span { class: "text-3xl", "🎤" }
                    h1 {
                        class: "text-2xl font-bold tracking-tight gradient-text-hero",
                        "Wi-Fi Party"
                    }
                }
                p {
                    class: "text-xs text-slate-400 font-medium ml-1",
                    "LOCAL AUDIO SHARING"
                }
            }

            div {
                class: "px-8 py-4",
                div {
                    class: "flex items-center gap-3 p-3 bg-slate-800/40 rounded-xl border border-slate-700/50",
                    div {
                        class: "relative w-3 h-3",
                        div { class: "absolute inset-0 rounded-full {status_bg} opacity-75 animate-ping" }
                        div { class: "relative w-3 h-3 rounded-full {status_bg}" }
                    }
                    div {
                        class: "flex flex-col",
                        span { class: "text-xs text-slate-400 font-semibold uppercase tracking-wider", "Status" }
                        span { class: "text-sm font-bold {status_color}", "{status_text}" }
                    }
                }
            }

            div {
                class: "flex-1 overflow-y-auto px-8 py-4 space-y-8",
                
                SelfAudioControls {
                    mic_enabled: mic_enabled,
                    mic_volume: mic_volume,
                    mic_audio_level: mic_audio_level,
                    loopback_enabled: loopback_enabled,
                }

                NetworkStatsCompact {}
            }

            div {
                class: "p-6 border-t border-slate-800/50 text-center text-xs text-slate-500",
                "v0.1.0 • UDP Multicast"
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn SelfAudioControls(
    mic_enabled: bool,
    mic_volume: f32,
    mic_audio_level: u32,
    loopback_enabled: bool,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    
    let state_mic = state_arc.clone();
    let on_mic_toggle = move |_| {
        let current = state_mic.mic_enabled.load(std::sync::atomic::Ordering::Relaxed);
        state_mic.mic_enabled.store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let state_vol = state_arc.clone();
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            if let Ok(mut vol) = state_vol.mic_volume.lock() {
                *vol = value_str / 100.0;
            }
        }
    };

    let state_loop = state_arc.clone();
    let on_loopback_toggle = move |_| {
        let current = state_loop.loopback_enabled.load(std::sync::atomic::Ordering::Relaxed);
        state_loop.loopback_enabled.store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    rsx! {
        div {
            class: "space-y-6",
            
            div {
                class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-4",
                "Audio Settings"
            }

            div {
                class: "grid grid-cols-2 gap-3",
                
                button {
                    class: format!(
                        "p-4 rounded-2xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}", 
                        if mic_enabled { "bg-emerald-500/10 border-emerald-500/50 text-emerald-400 hover:bg-emerald-500/20" }
                        else { "bg-rose-500/10 border-rose-500/50 text-rose-400 hover:bg-rose-500/20" }
                    ),
                    onclick: on_mic_toggle,
                    div { class: "text-2xl", if mic_enabled { "🎙️" } else { "🔇" } }
                    span { class: "text-xs font-bold", if mic_enabled { "Active" } else { "Muted" } }
                }

                button {
                    class: format!(
                        "p-4 rounded-2xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}", 
                        if loopback_enabled { "bg-indigo-500/10 border-indigo-500/50 text-indigo-400 hover:bg-indigo-500/20" } 
                        else { "bg-slate-800 border-slate-700 text-slate-400 hover:bg-slate-700 hover:text-slate-300" }
                    ),
                    onclick: on_loopback_toggle,
                    div { class: "text-2xl", "🎧" }
                    span { class: "text-xs font-bold", if loopback_enabled { "Loopback On" } else { "Loopback Off" } }
                }
            }

            div {
                div {
                    class: "flex justify-between text-xs mb-2",
                    span { class: "text-slate-400", "Input Gain" }
                    span { class: "font-mono font-bold text-slate-200", "{(mic_volume * 100.0) as i32}%" }
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

            div {
                div {
                    class: "flex justify-between text-xs mb-2",
                    span { class: "text-slate-400", "Mic Level" }
                }
                div {
                    class: "h-2 bg-slate-800 rounded-full overflow-hidden relative",
                    // Full-width gradient layer (green->yellow->red from 0%->100%)
                    div {
                        class: "absolute inset-0 bg-gradient-to-r from-emerald-500 via-yellow-400 to-rose-500",
                    }
                    // Overlay that hides the gradient beyond the current level
                    div {
                        class: "absolute inset-0 bg-slate-800 transition-all duration-75",
                        style: "left: {mic_audio_level}%",
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn NetworkStatsCompact() -> Element {
    rsx! {
        div {
            class: "space-y-3 pt-4 border-t border-slate-800/50",
            div {
                class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-2",
                "Network Health"
            }
            
            div {
                class: "grid grid-cols-2 gap-2",
                StatBox { label: "Latency", value: "~20ms", color: "text-emerald-400" }
                StatBox { label: "Loss", value: "0%", color: "text-indigo-400" }
                StatBox { label: "Jitter", value: "2ms", color: "text-yellow-400" }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn StatBox(label: &'static str, value: &'static str, color: &'static str) -> Element {
    rsx! {
        div {
            class: "bg-slate-800/30 p-2 rounded-lg border border-slate-700/30",
            div { class: "text-[10px] text-slate-500 uppercase", "{label}" }
            div { class: "text-sm font-mono font-bold {color}", "{value}" }
        }
    }
}
