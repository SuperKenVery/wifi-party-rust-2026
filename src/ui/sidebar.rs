//! Sidebar components for audio controls and status display.

use crate::state::AppState;
use dioxus::prelude::*;
use std::sync::Arc;

#[allow(non_snake_case)]
#[component]
pub fn Sidebar(
    mic_enabled: bool,
    mic_volume: f32,
    mic_audio_level: u32,
    loopback_enabled: bool,
    system_audio_enabled: bool,
    system_audio_level: u32,
) -> Element {
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
                class: "flex-1 overflow-y-auto px-8 py-4 space-y-8",
                
                SelfAudioControls {
                    mic_enabled: mic_enabled,
                    mic_volume: mic_volume,
                    mic_audio_level: mic_audio_level,
                    loopback_enabled: loopback_enabled,
                    system_audio_enabled: system_audio_enabled,
                    system_audio_level: system_audio_level,
                }
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
    system_audio_enabled: bool,
    system_audio_level: u32,
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

    let state_sys = state_arc.clone();
    let on_system_audio_toggle = move |_| {
        let current = state_sys.system_audio_enabled.load(std::sync::atomic::Ordering::Relaxed);
        state_sys.system_audio_enabled.store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    rsx! {
        div {
            class: "space-y-6",
            
            div {
                class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-4",
                "Audio Settings"
            }

            div {
                class: "grid grid-cols-3 gap-3",
                
                // Mic toggle button
                button {
                    class: format!(
                        "p-4 rounded-2xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}", 
                        if mic_enabled { "bg-emerald-500/10 border-emerald-500/50 text-emerald-400 hover:bg-emerald-500/20" }
                        else { "bg-rose-500/10 border-rose-500/50 text-rose-400 hover:bg-rose-500/20" }
                    ),
                    onclick: on_mic_toggle,
                    div { class: "text-2xl", if mic_enabled { "🎙️" } else { "🔇" } }
                    span { class: "text-xs font-bold", if mic_enabled { "Mic On" } else { "Mic Off" } }
                }

                // Loopback toggle button
                button {
                    class: format!(
                        "p-4 rounded-2xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}", 
                        if loopback_enabled { "bg-indigo-500/10 border-indigo-500/50 text-indigo-400 hover:bg-indigo-500/20" } 
                        else { "bg-slate-800 border-slate-700 text-slate-400 hover:bg-slate-700 hover:text-slate-300" }
                    ),
                    onclick: on_loopback_toggle,
                    div { class: "text-2xl", "🎧" }
                    span { class: "text-xs font-bold", if loopback_enabled { "Loopback" } else { "No Loop" } }
                }

                // System audio toggle button
                button {
                    class: format!(
                        "p-4 rounded-2xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}", 
                        if system_audio_enabled { "bg-purple-500/10 border-purple-500/50 text-purple-400 hover:bg-purple-500/20" } 
                        else { "bg-slate-800 border-slate-700 text-slate-400 hover:bg-slate-700 hover:text-slate-300" }
                    ),
                    onclick: on_system_audio_toggle,
                    div { class: "text-2xl", "🔊" }
                    span { class: "text-xs font-bold", if system_audio_enabled { "Sharing" } else { "Not Share" } }
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
                    div {
                        class: "absolute inset-0",
                        style: "background: linear-gradient(to right, #22c55e 0%, #22c55e 50%, #eab308 75%, #ef4444 100%)",
                    }
                    div {
                        class: "absolute inset-0 bg-slate-800 transition-all duration-75",
                        style: "left: {mic_audio_level}%",
                    }
                }
            }

            div {
                div {
                    class: "flex justify-between text-xs mb-2",
                    span { class: "text-slate-400", "System Audio Level" }
                }
                div {
                    class: "h-2 bg-slate-800 rounded-full overflow-hidden relative",
                    div {
                        class: "absolute inset-0",
                        style: "background: linear-gradient(to right, #22c55e 0%, #22c55e 50%, #eab308 75%, #ef4444 100%)",
                    }
                    div {
                        class: "absolute inset-0 bg-slate-800 transition-all duration-75",
                        style: "left: {system_audio_level}%",
                    }
                }
            }
        }
    }
}
