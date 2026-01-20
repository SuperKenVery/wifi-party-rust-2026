//! Participant display components showing connected hosts.

use crate::state::HostInfo;
use dioxus::prelude::*;

#[allow(non_snake_case)]
#[component]
pub fn MainContent(hosts: Vec<HostInfo>) -> Element {
    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",
            
            div {
                class: "h-20 px-8 flex items-center justify-between z-10",
                div {
                    class: "flex items-center gap-4",
                    h2 { class: "text-xl font-bold text-white", "Participants" }
                    span {
                        class: "px-2.5 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 text-xs font-bold border border-indigo-500/30",
                        "{hosts.len()} Active"
                    }
                }
                
                div {
                    class: "flex gap-2",
                    button {
                        class: "w-8 h-8 rounded-full bg-slate-800 flex items-center justify-center text-slate-400 hover:text-white transition-colors",
                        "⚙️"
                    }
                }
            }

            div {
                class: "flex-1 overflow-y-auto p-8 pt-0",
                
                if hosts.is_empty() {
                    div {
                        class: "h-full flex flex-col items-center justify-center text-slate-400",
                        div {
                            class: "w-24 h-24 bg-slate-800/50 rounded-full flex items-center justify-center text-4xl mb-6",
                            "📡"
                        }
                        h3 { class: "text-lg font-medium text-slate-200 mb-2", "No Participants Yet" }
                        p { class: "text-sm max-w-xs text-center text-slate-400", "Wait for others to join the party on your local network." }
                    }
                } else {
                    div {
                        class: "flex flex-col gap-6 pb-20",
                        for host in hosts {
                            HostCard { host }
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
    rsx! {
        div {
            class: "glass-card p-5 rounded-2xl relative group",
            
            div {
                class: "flex items-start justify-between mb-4",
                div {
                    class: "flex items-center gap-3",
                    div {
                        class: "w-10 h-10 rounded-full bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center text-white font-bold shadow-lg shadow-indigo-500/20",
                        "U"
                    }
                    div {
                        class: "flex flex-col",
                        span { class: "font-bold text-sm text-slate-200", "{host.id.to_string()}" }
                        div {
                            class: "flex items-center gap-1.5",
                            span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500" }
                            span { class: "text-[10px] font-medium text-slate-400 uppercase", "Connected" }
                        }
                    }
                }
            }

            div {
                class: "space-y-2",
                for stream in &host.streams {
                    StreamIndicator {
                        stream_id: stream.stream_id.clone(),
                        packet_loss: stream.packet_loss,
                        target_latency: stream.target_latency,
                    }
                }
                if host.streams.is_empty() {
                    div {
                        class: "text-xs text-slate-500 italic",
                        "No active streams"
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn StreamIndicator(
    stream_id: String,
    packet_loss: f32,
    target_latency: f32,
) -> Element {
    let icon = if stream_id == "Mic" { "🎙️" } else { "🔊" };
    let packet_loss_pct = (packet_loss * 100.0) as i32;
    let target_lat = target_latency as i32;

    let loss_color = if packet_loss < 0.02 {
        "text-emerald-400"
    } else if packet_loss < 0.10 {
        "text-yellow-400"
    } else {
        "text-red-400"
    };

    rsx! {
        div {
            class: "flex items-center gap-3",
            span { class: "text-sm", "{icon}" }
            span { class: "text-xs text-slate-400 w-16", "{stream_id}" }
            div {
                class: "flex gap-4 text-[10px]",
                span { class: "text-slate-500",
                    "Loss: "
                    span { class: "{loss_color}", "{packet_loss_pct}%" }
                }
                span { class: "text-slate-500",
                    "Target: "
                    span { class: "text-indigo-400", "{target_lat} frames" }
                }
            }
        }
    }
}
