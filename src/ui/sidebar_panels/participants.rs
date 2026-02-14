//! Participant display components showing connected hosts.

use crate::party::StreamSnapshot;
use crate::state::{AppState, HostId, HostInfo};
use dioxus::prelude::*;
use std::sync::Arc;

use super::PanelHeader;

#[allow(non_snake_case)]
#[component]
pub fn MainContent(
    hosts: Vec<HostInfo>,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
    let badge = format!("{} Active", hosts.len());

    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",

            PanelHeader {
                title: "Participants",
                badge: Some(badge),
                on_back,
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
                    host_id: host.id,
                    stream_id: stream.stream_id.clone(),
                    packet_loss: stream.packet_loss,
                    target_latency: stream.target_latency,
                    audio_level: stream.audio_level,
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
    host_id: HostId,
    stream_id: String,
    packet_loss: f32,
    target_latency: f32,
    audio_level: u32,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let mut snapshots = use_signal(Vec::<StreamSnapshot>::new);
    let mut show_graph = use_signal(|| false);

    let icon = if stream_id == "Mic" {
        "🎙️"
    } else {
        "🔊"
    };
    let packet_loss_pct = (packet_loss * 100.0) as i32;
    let target_lat = target_latency as i32;

    let loss_color = if packet_loss < 0.02 {
        "text-emerald-400"
    } else if packet_loss < 0.10 {
        "text-yellow-400"
    } else {
        "text-red-400"
    };

    let level_color = if audio_level < 30 {
        "bg-emerald-500"
    } else if audio_level < 70 {
        "bg-yellow-500"
    } else {
        "bg-red-500"
    };

    let stream_id_clone = stream_id.clone();
    let on_debug_click = move |_| {
        let new_show = !show_graph();
        show_graph.set(new_show);
        if new_show {
            let fetched = state_arc.stream_snapshots(host_id, &stream_id_clone);
            snapshots.set(fetched);
        }
    };

    rsx! {
        div {
            class: "flex flex-col gap-2",

            div {
                class: "flex items-center gap-3 w-full",
                span { class: "text-sm flex-shrink-0", "{icon}" }
                span { class: "text-xs text-slate-400 w-16 flex-shrink-0", "{stream_id}" }

                div {
                    class: "flex-1 h-1.5 bg-slate-700 rounded-full overflow-hidden",
                    title: "Audio Level: {audio_level}%",
                    div {
                        class: "h-full {level_color} transition-all duration-75",
                        style: "width: {audio_level}%",
                    }
                }

                div {
                    class: "flex gap-4 text-[10px] flex-shrink-0",
                    span { class: "text-slate-500",
                        "Loss: "
                        span { class: "{loss_color}", "{packet_loss_pct}%" }
                    }
                    span { class: "text-slate-500",
                        "Target: "
                        span { class: "text-indigo-400", "{target_lat} frames" }
                    }
                }

                button {
                    class: "ml-2 w-6 h-6 flex-shrink-0 rounded bg-slate-700 hover:bg-slate-600 flex items-center justify-center text-xs text-slate-400 hover:text-white transition-colors",
                    title: "Toggle packet graph",
                    onclick: on_debug_click,
                    "📊"
                }
            }

            if show_graph() {
                PacketGraph { snapshots: snapshots() }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn PacketGraph(snapshots: Vec<StreamSnapshot>) -> Element {
    if snapshots.is_empty() {
        return rsx! {
            div {
                class: "text-xs text-slate-500 italic pl-6",
                "No snapshot data available"
            }
        };
    }

    let view_width = 500.0;
    let view_height = 420.0;
    let left_margin = 50.0;
    let right_margin = 10.0;
    let top_margin = 10.0;
    let bottom_margin = 25.0;
    let graph_width = view_width - left_margin - right_margin;
    let graph_height = view_height - top_margin - bottom_margin;

    let min_seq = snapshots.iter().map(|s| s.read_seq).min().unwrap_or(0);
    let max_seq = snapshots.iter().map(|s| s.write_seq).max().unwrap_or(1);
    let seq_range = (max_seq - min_seq).max(1) as f64;

    let num_snapshots = snapshots.len();
    let x_scale = if num_snapshots > 1 {
        graph_width / (num_snapshots - 1) as f64
    } else {
        graph_width
    };
    let y_scale = graph_height / seq_range;

    let seq_to_y = |seq: u64| -> f64 {
        top_margin + graph_height - (seq.saturating_sub(min_seq) as f64 * y_scale)
    };

    let idx_to_x = |i: usize| -> f64 { left_margin + i as f64 * x_scale };

    let write_points: String = snapshots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let x = idx_to_x(i);
            let y = seq_to_y(s.write_seq);
            format!("{:.1},{:.1}", x, y)
        })
        .collect::<Vec<_>>()
        .join(" ");

    let read_points: String = snapshots
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let x = idx_to_x(i);
            let y = seq_to_y(s.read_seq);
            format!("{:.1},{:.1}", x, y)
        })
        .collect::<Vec<_>>()
        .join(" ");

    let mut slot_dots: Vec<(f64, f64, bool)> = Vec::new();
    for (i, snapshot) in snapshots.iter().enumerate() {
        let x = idx_to_x(i);
        for (j, &has_data) in snapshot.slot_status.iter().enumerate() {
            let seq = snapshot.read_seq + j as u64;
            let y = seq_to_y(seq);
            slot_dots.push((x, y, has_data));
        }
    }

    let time_per_snapshot_ms = 1000.0 / num_snapshots as f64;
    let x_labels: Vec<(f64, String)> = [0.0, 0.25, 0.5, 0.75, 1.0]
        .iter()
        .map(|&frac| {
            let idx = ((num_snapshots - 1) as f64 * frac) as usize;
            let x = idx_to_x(idx);
            let time_ms = (num_snapshots - 1 - idx) as f64 * time_per_snapshot_ms;
            let label = format!("-{:.0}ms", time_ms);
            (x, label)
        })
        .collect();

    let y_tick_count = 5;
    let y_labels: Vec<(f64, String)> = (0..=y_tick_count)
        .map(|i| {
            let frac = i as f64 / y_tick_count as f64;
            let seq = min_seq + (seq_range * frac) as u64;
            let y = seq_to_y(seq);
            (y, format!("{}", seq))
        })
        .collect();

    rsx! {
        div {
            class: "mt-1 p-2 bg-slate-800 rounded-lg w-full",
            svg {
                class: "w-full",
                height: "{view_height}",
                view_box: "0 0 {view_width} {view_height}",
                preserve_aspect_ratio: "xMidYMid meet",

                // Y-axis
                line {
                    x1: "{left_margin}",
                    y1: "{top_margin}",
                    x2: "{left_margin}",
                    y2: "{top_margin + graph_height}",
                    stroke: "#475569",
                    stroke_width: "1",
                }

                // X-axis
                line {
                    x1: "{left_margin}",
                    y1: "{top_margin + graph_height}",
                    x2: "{left_margin + graph_width}",
                    y2: "{top_margin + graph_height}",
                    stroke: "#475569",
                    stroke_width: "1",
                }

                // Y-axis labels
                for (y, label) in &y_labels {
                    text {
                        x: "{left_margin - 5.0}",
                        y: "{y}",
                        text_anchor: "end",
                        dominant_baseline: "middle",
                        font_size: "9",
                        fill: "#64748b",
                        "{label}"
                    }
                    line {
                        x1: "{left_margin - 3.0}",
                        y1: "{y}",
                        x2: "{left_margin}",
                        y2: "{y}",
                        stroke: "#475569",
                        stroke_width: "1",
                    }
                }

                // X-axis labels
                for (x, label) in &x_labels {
                    text {
                        x: "{x}",
                        y: "{top_margin + graph_height + 15.0}",
                        text_anchor: "middle",
                        font_size: "9",
                        fill: "#64748b",
                        "{label}"
                    }
                    line {
                        x1: "{x}",
                        y1: "{top_margin + graph_height}",
                        x2: "{x}",
                        y2: "{top_margin + graph_height + 3.0}",
                        stroke: "#475569",
                        stroke_width: "1",
                    }
                }

                // Slot dots (draw first so lines are on top)
                for (x, y, has_data) in &slot_dots {
                    circle {
                        cx: "{x}",
                        cy: "{y}",
                        r: "2",
                        fill: if *has_data { "#22c55e" } else { "#ef4444" },
                    }
                }

                // Write seq line
                polyline {
                    points: "{write_points}",
                    fill: "none",
                    stroke: "#818cf8",
                    stroke_width: "1.5",
                }

                // Read seq line
                polyline {
                    points: "{read_points}",
                    fill: "none",
                    stroke: "#a78bfa",
                    stroke_width: "1.5",
                }
            }

            div {
                class: "flex gap-4 mt-1 text-[9px] text-slate-500 justify-center",
                span {
                    class: "flex items-center gap-1",
                    span { class: "w-3 h-0.5 bg-indigo-400 inline-block" }
                    "write_seq"
                }
                span {
                    class: "flex items-center gap-1",
                    span { class: "w-3 h-0.5 bg-purple-400 inline-block" }
                    "read_seq"
                }
                span {
                    class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-emerald-500 inline-block" }
                    "filled"
                }
                span {
                    class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-red-500 inline-block" }
                    "missing"
                }
                span {
                    class: "text-slate-600",
                    "({num_snapshots} samples)"
                }
            }
        }
    }
}
