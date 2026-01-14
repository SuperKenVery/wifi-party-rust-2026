//! Participant display components showing connected hosts.

use crate::state::HostInfo;
use dioxus::prelude::*;

#[allow(non_snake_case)]
#[component]
pub fn MainContent(hosts: Vec<HostInfo>) -> Element {
    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-[url('https://grainy-gradients.vercel.app/noise.svg')] bg-opacity-5",
            
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
                        class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6 pb-20",
                        for host in hosts {
                            HostCard { host: host.clone() }
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
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            let _volume = value_str / 100.0;
        }
    };

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
                class: "space-y-3",
                
                div {
                    class: "flex items-center justify-between text-xs text-slate-400 mb-1",
                    span { "Volume" }
                    span { class: "text-slate-300", "{(host.volume * 100.0) as i32}%" }
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

            div {
                class: "mt-4 pt-4 border-t border-white/5 flex items-center justify-between text-xs",
                div {
                    class: "flex gap-3",
                    span { class: "text-slate-500", "Loss: <span class=\"text-slate-300 ml-1\">{(host.packet_loss * 100.0) as i32}%</span>" }
                    span { class: "text-slate-500", "Ping: <span class=\"text-emerald-400 ml-1\">20ms</span>" }
                }
            }
        }
    }
}
