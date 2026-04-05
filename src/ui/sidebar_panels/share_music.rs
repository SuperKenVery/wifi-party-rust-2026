use crate::music_provider::MusicProvider;
use crate::party::SyncedStreamState;
use crate::state::AppState;
use dioxus::prelude::*;
use std::sync::Arc;

use super::PanelHeader;

#[derive(Clone, PartialEq)]
struct SenderProgressInfo {
    frames_sent: u64,
    total_frames: u64,
    samples_played: u64,
    total_samples: u64,
    sample_rate: u32,
}

#[derive(Clone, PartialEq)]
struct ReceiverProgressInfo {
    frames_received: u64,
    highest_seq: u64,
    total_frames: u64,
    samples_played: u64,
    total_samples: u64,
    sample_rate: u32,
}

#[allow(non_snake_case)]
#[component]
pub fn ShareMusicPanel(
    active_streams: Vec<SyncedStreamState>,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let mut selected_provider = use_signal(|| 0usize);

    let providers: Vec<Box<dyn MusicProvider>> = state_arc
        .music_provider_factories
        .iter()
        .map(|f| f(state_arc.clone()))
        .collect();

    let progress = state_arc.music_progress.clone();
    let is_streaming = progress
        .is_streaming
        .load(std::sync::atomic::Ordering::Relaxed);
    let streaming_current = progress
        .streaming_current
        .load(std::sync::atomic::Ordering::Relaxed);
    let streaming_total = progress
        .streaming_total
        .load(std::sync::atomic::Ordering::Relaxed);
    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",

            PanelHeader { title: "Share Music", on_back }

            div {
                class: "flex-1 overflow-y-auto p-8 pt-0",

                div {
                    class: "max-w-2xl",

                    div {
                        class: "glass-card p-6 rounded-2xl space-y-6",

                        p {
                            class: "text-sm text-slate-400",
                            "Share local files or Apple Music tracks with all participants. Playback stays synchronized across connected devices using shared party time."
                        }

                        div {
                            class: "space-y-2",
                            label {
                                class: "text-sm text-slate-400 font-medium",
                                "Source:"
                            }
                            select {
                                class: "w-full p-3 rounded-xl bg-slate-800 border border-slate-700 text-white text-sm focus:outline-none focus:border-pink-500/50",
                                value: "{selected_provider()}",
                                onchange: move |evt| {
                                    if let Ok(idx) = evt.value().parse::<usize>() {
                                        selected_provider.set(idx);
                                    }
                                },
                                for (i, provider) in providers.iter().enumerate() {
                                    option {
                                        value: "{i}",
                                        "{provider.name()}"
                                    }
                                }
                            }
                        }

                        { providers[selected_provider()].render() }

                        if is_streaming || !active_streams.is_empty() {
                            div {
                                class: "space-y-4",
                                for stream in active_streams {
                                    div {
                                        key: "{stream.stream_id}",
                                        class: "p-4 rounded-xl bg-emerald-500/10 border border-emerald-500/30",
                                        div {
                                            class: "flex items-center justify-between mb-2",
                                            div {
                                                class: "flex items-center gap-2",
                                                span { class: "text-emerald-400 text-lg", if stream.progress.is_playing { "▶" } else { "⏸" } }
                                                span { class: "text-sm text-emerald-300 font-medium", "Now playing:" }
                                            }
                                            span {
                                                class: "text-xs text-slate-400",
                                                if stream.is_local_sender { "(Sender)" } else { "(Receiver)" }
                                            }
                                        }
                                        {
                                            let meta = &stream.meta;
                                            let samples_played = stream.progress.samples_played;
                                            let total_samples = stream.meta.total_samples;
                                            let sample_rate = stream.meta.codec_params.sample_rate;
                                            rsx! {
                                                p {
                                                    class: "text-sm text-emerald-400/80 mb-2 truncate",
                                                    "{meta.file_name}"
                                                }

                                                {
                                                    if stream.is_local_sender {
                                                        let total = streaming_total.max(meta.total_frames).max(1);
                                                        let sender_info = SenderProgressInfo {
                                                            frames_sent: streaming_current,
                                                            total_frames: total,
                                                            samples_played,
                                                            total_samples,
                                                            sample_rate,
                                                        };
                                                        rsx! { SenderProgressBar { info: sender_info } }
                                                    } else {
                                                        let total = meta.total_frames.max(1);
                                                        let receiver_info = ReceiverProgressInfo {
                                                            frames_received: stream.progress.buffered_frames,
                                                            highest_seq: stream.progress.highest_seq_received,
                                                            total_frames: total,
                                                            samples_played,
                                                            total_samples,
                                                            sample_rate,
                                                        };
                                                        rsx! { ReceiverProgressBar { info: receiver_info } }
                                                    }
                                                }

                                                // Controls
                                                div {
                                                class: "flex items-center justify-center gap-4 mt-4",
                                                button {
                                                    class: "p-2 rounded-full hover:bg-emerald-500/20 text-emerald-400 transition-colors",
                                                    onclick: {
                                                        let state = state_arc.clone();
                                                        let stream_id = stream.stream_id;
                                                        let current_ms = samples_played * 1000 / sample_rate as u64;
                                                        move |_| {
                                                            let _ = state.seek_music(stream_id, current_ms.saturating_sub(10_000));
                                                        }
                                                    },
                                                    "⏪"
                                                }
                                                button {
                                                    class: "p-3 rounded-full bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-400 transition-colors",
                                                    onclick: {
                                                        let state = state_arc.clone();
                                                        let stream_id = stream.stream_id;
                                                        let is_playing = stream.progress.is_playing;
                                                        move |_| {
                                                            if is_playing {
                                                                let _ = state.pause_music(stream_id);
                                                            } else {
                                                                let _ = state.resume_music(stream_id);
                                                            }
                                                        }
                                                    },
                                                    if stream.progress.is_playing { "⏸" } else { "▶" }
                                                }
                                                button {
                                                    class: "p-2 rounded-full hover:bg-emerald-500/20 text-emerald-400 transition-colors",
                                                    onclick: {
                                                        let state = state_arc.clone();
                                                        let stream_id = stream.stream_id;
                                                        let current_ms = samples_played * 1000 / sample_rate as u64;
                                                        move |_| {
                                                            let _ = state.seek_music(stream_id, current_ms + 10_000);
                                                        }
                                                    },
                                                    "⏩"
                                                }
                                                }
                                            }
                                        }
                                    }
                                }

                            }
                        }

                        div {
                            class: "text-sm text-slate-500 space-y-2 pt-4 border-t border-slate-800",
                            p { class: "font-medium text-slate-400", "Supported formats:" }
                            p { "MP3, FLAC, WAV, OGG, M4A, AAC" }
                        }
                    }
                }
            }
        }
    }
}

fn format_time(total_ms: u64) -> String {
    let secs = total_ms / 1000;
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}

#[allow(non_snake_case)]
#[component]
fn SenderProgressBar(info: SenderProgressInfo) -> Element {
    let total_frames = info.total_frames.max(1);
    let total_samples = info.total_samples.max(1);
    let sent_pct = (info.frames_sent as f64 / total_frames as f64 * 100.0) as u32;
    let played_pct = (info.samples_played as f64 / total_samples as f64 * 100.0) as u32;

    let current_time = format_time(info.samples_played * 1000 / info.sample_rate as u64);
    let total_time = format_time(total_samples * 1000 / info.sample_rate as u64);

    rsx! {
        div {
            class: "space-y-1",
            div {
                class: "relative w-full h-3 bg-slate-600 rounded-full overflow-hidden",
                div {
                    class: "absolute left-0 top-0 h-full bg-sky-500",
                    style: "width: {sent_pct}%",
                }
                div {
                    class: "absolute top-0 h-full w-1 bg-white shadow-md",
                    style: "left: calc({played_pct}% - 2px)",
                }
            }
            div {
                class: "flex justify-between text-xs text-slate-400",
                span { "{current_time} / {total_time}" }
                span { class: "text-slate-500", "sent:{info.frames_sent} tot:{total_frames}" }
            }
            div {
                class: "flex gap-3 text-xs text-slate-500",
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-sky-500" }
                    "Sent"
                }
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-white" }
                    "Playback"
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn ReceiverProgressBar(info: ReceiverProgressInfo) -> Element {
    let total_frames = info.total_frames.max(1);
    let total_samples = info.total_samples.max(1);
    let received_pct = (info.frames_received as f64 / total_frames as f64 * 100.0) as u32;
    let missing_count = info.highest_seq.saturating_sub(info.frames_received);
    let missing_pct = (missing_count as f64 / total_frames as f64 * 100.0) as u32;
    let played_pct = (info.samples_played as f64 / total_samples as f64 * 100.0) as u32;

    let current_time = format_time(info.samples_played * 1000 / info.sample_rate as u64);
    let total_time = format_time(total_samples * 1000 / info.sample_rate as u64);

    rsx! {
        div {
            class: "space-y-1",
            div {
                class: "relative w-full h-3 bg-slate-600 rounded-full overflow-hidden",
                div {
                    class: "absolute left-0 top-0 h-full bg-emerald-500",
                    style: "width: {received_pct}%",
                }
                div {
                    class: "absolute top-0 h-full bg-rose-500",
                    style: "left: {received_pct}%; width: {missing_pct}%",
                }
                div {
                    class: "absolute top-0 h-full w-1 bg-white shadow-md",
                    style: "left: calc({played_pct}% - 2px)",
                }
            }
            div {
                class: "flex justify-between text-xs text-slate-400",
                span { "{current_time} / {total_time}" }
                div {
                    class: "flex gap-3",
                    span { class: "flex items-center gap-1",
                        span { class: "w-2 h-2 rounded-full bg-emerald-500" }
                        "Received"
                    }
                    span { class: "flex items-center gap-1",
                        span { class: "w-2 h-2 rounded-full bg-rose-500" }
                        "Missing"
                    }
                }
            }
        }
    }
}
