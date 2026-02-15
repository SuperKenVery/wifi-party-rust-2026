use crate::party::SyncedStreamState;
use crate::state::AppState;
use dioxus::prelude::*;
use std::sync::Arc;
use tracing::error;

use super::PanelHeader;

#[derive(Clone, PartialEq)]
struct SenderProgressInfo {
    frames_sent: u64,
    frames_encoded: u64,
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

    let progress = state_arc.music_progress.clone();
    let is_encoding = progress
        .is_encoding
        .load(std::sync::atomic::Ordering::Relaxed);
    let is_streaming = progress
        .is_streaming
        .load(std::sync::atomic::Ordering::Relaxed);
    let encoding_current = progress
        .encoding_current
        .load(std::sync::atomic::Ordering::Relaxed);
    let encoding_total = progress
        .encoding_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let streaming_current = progress
        .streaming_current
        .load(std::sync::atomic::Ordering::Relaxed);
    let streaming_total = progress
        .streaming_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let file_name = progress.file_name.lock().unwrap().clone();



    let encoding_percent = if encoding_total > 0 {
        (encoding_current as f64 / encoding_total as f64 * 100.0) as u32
    } else {
        0
    };

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
                            "Share a music file with all participants. The audio will be synchronized across all connected devices using NTP-like time synchronization."
                        }

                        { file_select_button(state_arc.clone()) }

                        if is_encoding {
                            div {
                                class: "p-4 rounded-xl bg-amber-500/10 border border-amber-500/30",
                                div {
                                    class: "flex items-center justify-between mb-2",
                                    div {
                                        class: "flex items-center gap-2",
                                        span { class: "text-amber-400 text-lg animate-pulse", "⏳" }
                                        span { class: "text-sm text-amber-300 font-medium", "Encoding..." }
                                    }
                                    span { class: "text-sm text-amber-400", "{encoding_percent}%" }
                                }
                                if let Some(ref name) = file_name {
                                    p {
                                        class: "text-sm text-amber-400/80 mb-2 truncate",
                                        "{name}"
                                    }
                                }
                                div {
                                    class: "w-full h-2 bg-slate-700 rounded-full overflow-hidden",
                                    div {
                                        class: "h-full bg-amber-500 transition-all duration-300",
                                        style: "width: {encoding_percent}%",
                                    }
                                }
                            }
                        }

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
                                                            frames_encoded: encoding_current,
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

                                // Show encoding progress if we are the sender
                                if is_encoding {
                                    div {
                                        class: "p-4 rounded-xl bg-amber-500/10 border border-amber-500/30",
                                        div {
                                            class: "flex items-center justify-between mb-2",
                                            div {
                                                class: "flex items-center gap-2",
                                                span { class: "text-amber-400 text-lg animate-pulse", "⏳" }
                                                span { class: "text-sm text-amber-300 font-medium", "Buffering to network..." }
                                            }
                                            span { class: "text-sm text-amber-400", "{encoding_percent}%" }
                                        }
                                        div {
                                            class: "w-full h-1 bg-slate-700 rounded-full overflow-hidden",
                                            div {
                                                class: "h-full bg-amber-500 transition-all duration-300",
                                                style: "width: {encoding_percent}%",
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
    let encoded_pct = (info.frames_encoded as f64 / total_frames as f64 * 100.0) as u32;
    let decoded_only_pct = encoded_pct.saturating_sub(sent_pct);
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
                    style: "width: {sent_pct}%",
                }
                div {
                    class: "absolute top-0 h-full bg-amber-500",
                    style: "left: {sent_pct}%; width: {decoded_only_pct}%",
                }
                div {
                    class: "absolute top-0 h-full w-1 bg-white shadow-md",
                    style: "left: calc({played_pct}% - 2px)",
                }
            }
            div {
                class: "flex justify-between text-xs text-slate-400",
                span { "{current_time} / {total_time}" }
                span { class: "text-slate-500", "sent:{info.frames_sent} enc:{info.frames_encoded} tot:{total_frames}" }
            }
            div {
                class: "flex gap-3 text-xs text-slate-500",
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-emerald-500" }
                    "Sent"
                }
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-amber-500" }
                    "Decoded"
                }
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-slate-600" }
                    "Pending"
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

const FILE_SELECT_BUTTON_CLASS: &str = "w-full p-6 rounded-2xl flex items-center justify-center gap-4 transition-all duration-200 border bg-pink-500/10 border-pink-500/50 text-pink-400 hover:bg-pink-500/20 cursor-pointer";

fn file_select_button(state: Arc<AppState>) -> Element {
    #[cfg(target_os = "android")]
    let on_click = {
        use crate::io::pick_audio_file;
        use tracing::info;

        move |_| {
            let state_clone = state.clone();
            spawn(async move {
                info!("Opening native file picker...");
                let Some(result) = pick_audio_file().await else {
                    info!("File picker returned None (cancelled or error)");
                    return;
                };

                info!("Got file: {}", result.name);
                if let Err(e) = state_clone.start_music_stream(result.data, result.name) {
                    error!("Failed to start music stream: {}", e);
                }
            });
        }
    };

    #[cfg(not(target_os = "android"))]
    let on_change = {
        move |evt: Event<FormData>| {
            let state_clone = state.clone();
            spawn(async move {
                let files = evt.files();
                let Some(file) = files.first() else {
                    return;
                };

                let file_name = file.name();
                let Ok(bytes) = file.read_bytes().await else {
                    error!("Failed to read file: {}", file_name);
                    return;
                };

                if let Err(e) = state_clone.start_music_stream(bytes.to_vec(), file_name) {
                    error!("Failed to start music stream: {}", e);
                }
            });
        }
    };

    #[cfg(target_os = "android")]
    return rsx! {
        button {
            class: FILE_SELECT_BUTTON_CLASS,
            onclick: on_click,
            div { class: "text-3xl", "🎵" }
            span { class: "text-lg font-bold", "Select Music File" }
        }
    };

    #[cfg(not(target_os = "android"))]
    rsx! {
        label {
            class: FILE_SELECT_BUTTON_CLASS,
            div { class: "text-3xl", "🎵" }
            span { class: "text-lg font-bold", "Select Music File" }
            input {
                r#type: "file",
                accept: ".mp3,.flac,.wav,.ogg,.m4a,.aac,audio/*",
                class: "hidden",
                onchange: on_change,
            }
        }
    }
}
