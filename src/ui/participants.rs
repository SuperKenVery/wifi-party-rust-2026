//! Participant display components showing connected hosts.

use crate::pipeline::node::{JitterEvent, TimelineSnapshot};
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
                        class: "flex flex-col gap-6 pb-20",
                        for host in hosts {
                            HostCard { 
                                key: "{host.id.to_string()}",
                                host: host.clone() 
                            }
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
                        audio_level: stream.audio_level,
                        packet_loss: stream.packet_loss,
                        jitter_latency_ms: stream.jitter_latency_ms,
                        hardware_latency_ms: stream.hardware_latency_ms,
                        timeline: stream.timeline.clone(),
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
    audio_level: f32,
    packet_loss: f32,
    jitter_latency_ms: f32,
    hardware_latency_ms: f32,
    timeline: TimelineSnapshot,
) -> Element {
    let mut snapshot: Signal<Option<TimelineSnapshot>> = use_signal(|| None);

    let level_pct = (audio_level * 100.0) as u32;
    let icon = if stream_id == "Mic" { "🎙️" } else { "🔊" };
    let packet_loss_pct = (packet_loss * 100.0) as i32;
    let jitter_ms = jitter_latency_ms as i32;
    let hw_ms = hardware_latency_ms as i32;

    let is_open = snapshot().is_some();

    rsx! {
        div {
            class: "space-y-1",
            div {
                class: "flex items-center gap-2",
                span { class: "text-sm", "{icon}" }
                span { class: "text-xs text-slate-400 w-12", "{stream_id}" }
                div {
                    class: "flex-1 h-2 bg-slate-800 rounded-full overflow-hidden relative",
                    div {
                        class: "absolute inset-0",
                        style: "background: linear-gradient(to right, #22c55e 0%, #22c55e 50%, #eab308 75%, #ef4444 100%)",
                    }
                    div {
                        class: "absolute inset-0 bg-slate-800 transition-all duration-75",
                        style: "left: {level_pct}%",
                    }
                }
                button {
                    class: "w-6 h-6 rounded bg-slate-700 hover:bg-slate-600 flex items-center justify-center text-xs transition-colors",
                    onclick: move |_| {
                        if snapshot().is_some() {
                            snapshot.set(None);
                        } else {
                            snapshot.set(Some(timeline.clone()));
                        }
                    },
                    if is_open { "▼" } else { "📊" }
                }
            }
            div {
                class: "flex gap-3 pl-7 text-[10px]",
                span { class: "text-slate-500",
                    "Loss: "
                    span { class: "text-slate-300", "{packet_loss_pct}%" }
                }
                span { class: "text-slate-500",
                    "Jitter: "
                    span { class: "text-emerald-400", "{jitter_ms}ms" }
                }
                span { class: "text-slate-500",
                    "HW: "
                    span { class: "text-indigo-400", "{hw_ms}ms" }
                }
            }
            
            if let Some(frozen_timeline) = snapshot() {
                TimelineGraph { timeline: frozen_timeline }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn TimelineGraph(timeline: TimelineSnapshot) -> Element {
    let mut selected_event: Signal<Option<JitterEvent>> = use_signal(|| None);

    let padding_left = 50;
    let padding_right = 30;
    let padding_top = 10;
    let padding_bottom = 25;
    let view_width = 1000;
    let view_height = 600;
    let graph_width = view_width - padding_left - padding_right;
    let graph_height = view_height - padding_top - padding_bottom;

    let entries = &timeline.entries;

    let min_time = entries.iter().map(|e| e.timestamp_ms).min().unwrap_or(0);
    let max_time = entries.iter().map(|e| e.timestamp_ms).max().unwrap_or(1000);
    let time_span = (max_time - min_time).max(1);

    let (min_seq, max_seq) = if entries.is_empty() {
        (0u64, 100u64)
    } else {
        let all_seqs: Vec<u64> = entries
            .iter()
            .flat_map(|e| [e.read_seq, e.write_seq])
            .collect();
        let min_s = all_seqs.iter().copied().min().unwrap_or(0);
        let max_s = all_seqs.iter().copied().max().unwrap_or(100);
        (min_s.saturating_sub(2), max_s + 2)
    };
    let seq_range = (max_seq - min_seq).max(1);

    let time_to_x = |t: u64| -> f64 {
        let ratio = (t - min_time) as f64 / time_span as f64;
        padding_left as f64 + ratio * graph_width as f64
    };

    let seq_to_y = |s: u64| -> f64 {
        let ratio = (s - min_seq) as f64 / seq_range as f64;
        padding_top as f64 + (1.0 - ratio) * graph_height as f64
    };

    let read_points: String = entries
        .iter()
        .map(|e| format!("{:.1},{:.1}", time_to_x(e.timestamp_ms), seq_to_y(e.read_seq)))
        .collect::<Vec<_>>()
        .join(" ");

    let write_points: String = entries
        .iter()
        .map(|e| format!("{:.1},{:.1}", time_to_x(e.timestamp_ms), seq_to_y(e.write_seq)))
        .collect::<Vec<_>>()
        .join(" ");

    let events_with_pos: Vec<(f64, &JitterEvent)> = entries
        .iter()
        .filter_map(|e| e.event.as_ref().map(|ev| (time_to_x(e.timestamp_ms), ev)))
        .collect();

    rsx! {
        div {
            class: "mt-2 bg-slate-900/50 rounded-lg p-2",
            
            svg {
                class: "w-full",
                style: "height: 400px;",
                view_box: "0 0 {view_width} {view_height}",
                preserve_aspect_ratio: "none",
                
                // Background grid
                for i in 0..=10 {
                    line {
                        x1: "{padding_left}",
                        y1: "{padding_top + i * graph_height / 10}",
                        x2: "{view_width - padding_right}",
                        y2: "{padding_top + i * graph_height / 10}",
                        stroke: "#1e293b",
                        stroke_width: "1",
                    }
                }
                
                // Y-axis labels
                text {
                    x: "5",
                    y: "{padding_top + 5}",
                    fill: "#64748b",
                    font_size: "12",
                    "{max_seq}"
                }
                text {
                    x: "5",
                    y: "{view_height - padding_bottom}",
                    fill: "#64748b",
                    font_size: "12",
                    "{min_seq}"
                }
                
                // X-axis labels
                {
                    let label = if time_span >= 1000 {
                        format!("-{:.1}s", time_span as f64 / 1000.0)
                    } else {
                        format!("-{}ms", time_span)
                    };
                    rsx! {
                        text {
                            x: "{padding_left}",
                            y: "{view_height - 5}",
                            fill: "#64748b",
                            font_size: "12",
                            "{label}"
                        }
                        text {
                            x: "{view_width - padding_right - 25}",
                            y: "{view_height - 5}",
                            fill: "#64748b",
                            font_size: "12",
                            "now"
                        }
                    }
                }
                
                // Draw dots for each seq between read_seq and write_seq at each timestamp
                for entry in entries.iter() {
                    {
                        let x = time_to_x(entry.timestamp_ms);
                        let read = entry.read_seq;
                        let write = entry.write_seq;
                        let buffer_state = &entry.buffer_state;
                        rsx! {
                            for (i, seq) in (read..=write).enumerate() {
                                {
                                    let y = seq_to_y(seq);
                                    let is_read = seq == read;
                                    let is_write = seq == write;
                                    let has_data = buffer_state.get(i).copied().unwrap_or(false);
                                    let color = if is_read {
                                        "#3b82f6"
                                    } else if is_write {
                                        "#22c55e"
                                    } else if has_data {
                                        "#475569"
                                    } else {
                                        "#ef4444"
                                    };
                                    let radius = if is_read || is_write { "4" } else { "2" };
                                    rsx! {
                                        circle {
                                            cx: "{x:.1}",
                                            cy: "{y:.1}",
                                            r: "{radius}",
                                            fill: "{color}",
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Event vertical lines
                for (x, event) in &events_with_pos {
                    {
                        let (color, width) = match event {
                            JitterEvent::MissingSeq { .. } => ("#ef4444", "2"),
                            JitterEvent::LowStabilityHoldBack { .. } => ("#f97316", "1"),
                            JitterEvent::HugeGapSkip { .. } => ("#a855f7", "1"),
                            JitterEvent::HighStabilityBump { .. } => ("#22c55e", "1"),
                        };
                        let event_clone = (*event).clone();
                        rsx! {
                            line {
                                x1: "{x:.1}",
                                y1: "{padding_top}",
                                x2: "{x:.1}",
                                y2: "{view_height - padding_bottom}",
                                stroke: "{color}",
                                stroke_width: "{width}",
                                stroke_dasharray: "4,2",
                                opacity: "0.5",
                                style: "cursor: pointer;",
                                onclick: move |_| selected_event.set(Some(event_clone.clone())),
                            }
                        }
                    }
                }
                
                // Write seq line (green)
                if !write_points.is_empty() {
                    polyline {
                        points: "{write_points}",
                        fill: "none",
                        stroke: "#22c55e",
                        stroke_width: "2",
                    }
                }
                
                // Read seq line (blue)
                if !read_points.is_empty() {
                    polyline {
                        points: "{read_points}",
                        fill: "none",
                        stroke: "#3b82f6",
                        stroke_width: "2",
                    }
                }
            }
            
            // Legend
            div {
                class: "flex items-center justify-between mt-2 text-xs text-slate-500",
                div {
                    class: "flex items-center gap-4",
                    span { class: "flex items-center gap-1",
                        span { class: "w-3 h-3 rounded-full bg-blue-500 inline-block" }
                        "read_seq"
                    }
                    span { class: "flex items-center gap-1",
                        span { class: "w-3 h-3 rounded-full bg-green-500 inline-block" }
                        "write_seq"
                    }
                    span { class: "flex items-center gap-1",
                        span { class: "w-2 h-2 rounded-full bg-slate-600 inline-block" }
                        "has data"
                    }
                    span { class: "flex items-center gap-1",
                        span { class: "w-2 h-2 rounded-full bg-red-500 inline-block" }
                        "missing"
                    }
                }
                if let Some(event) = selected_event() {
                    div {
                        class: "text-slate-400 cursor-pointer hover:text-slate-300",
                        onclick: move |_| selected_event.set(None),
                        {format_event_short(&event)}
                        " ✕"
                    }
                }
            }
        }
    }
}

fn format_event_short(event: &JitterEvent) -> String {
    match event {
        JitterEvent::MissingSeq { seq, stability } => {
            format!("miss #{} ({:.0}%)", seq, stability * 100.0)
        }
        JitterEvent::LowStabilityHoldBack { stability, latency } => {
            format!("hold ({:.0}%, lat={})", stability * 100.0, latency)
        }
        JitterEvent::HugeGapSkip { skip_amount, .. } => {
            format!("skip {} frames", skip_amount)
        }
        JitterEvent::HighStabilityBump { latency, .. } => {
            format!("bump (lat={})", latency)
        }
    }
}
