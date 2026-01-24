use crate::state::AppState;
use dioxus::prelude::*;
use std::sync::Arc;
use tracing::error;

#[allow(non_snake_case)]
#[component]
pub fn ShareMusicPanel() -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let mut is_picking = use_signal(|| false);

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
    let _streaming_current = progress
        .streaming_current
        .load(std::sync::atomic::Ordering::Relaxed);
    let _streaming_total = progress
        .streaming_total
        .load(std::sync::atomic::Ordering::Relaxed);
    let file_name = progress.file_name.lock().unwrap().clone();

    // Get active streams for playback control
    let active_streams = state_arc.synced_stream_states();

    let is_busy = is_encoding || is_streaming;

    let on_share_music = {
        let state = state_arc.clone();
        move |_| {
            is_picking.set(true);

            let state_clone = state.clone();
            spawn(async move {
                let file_handle = rfd::AsyncFileDialog::new()
                    .add_filter("Audio Files", &["mp3", "flac", "wav", "ogg", "m4a", "aac"])
                    .pick_file()
                    .await;

                if let Some(handle) = file_handle {
                    let path = handle.path().to_path_buf();

                    if let Err(e) = state_clone.start_music_stream(path) {
                        error!("Failed to start music stream: {}", e);
                    }
                }

                is_picking.set(false);
            });
        }
    };

    let encoding_percent = if encoding_total > 0 {
        (encoding_current as f64 / encoding_total as f64 * 100.0) as u32
    } else {
        0
    };

    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",

            div {
                class: "h-20 px-8 flex items-center justify-between z-10",
                div {
                    class: "flex items-center gap-4",
                    h2 { class: "text-xl font-bold text-white", "Share Music" }
                }
            }

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

                        button {
                            class: "w-full p-6 rounded-2xl flex items-center justify-center gap-4 transition-all duration-200 border bg-pink-500/10 border-pink-500/50 text-pink-400 hover:bg-pink-500/20 disabled:opacity-50 disabled:cursor-not-allowed",
                            onclick: on_share_music,
                            disabled: *is_picking.read() || is_busy,
                            div { class: "text-3xl", "🎵" }
                            span {
                                class: "text-lg font-bold",
                                if *is_picking.read() {
                                    "Selecting..."
                                } else if is_busy {
                                    "Busy..."
                                } else {
                                    "Select Music File"
                                }
                            }
                        }

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
                                            if let Some(meta) = &stream.meta {
                                                span {
                                                    class: "text-sm text-emerald-400",
                                                    "{stream.progress.frames_played * 100 / meta.total_frames.max(1)}%"
                                                }
                                            }
                                        }
                                        if let Some(meta) = &stream.meta {
                                            p {
                                                class: "text-sm text-emerald-400/80 mb-2 truncate",
                                                "{meta.file_name}"
                                            }

                                            // Progress Bar / Seek
                                            div {
                                                class: "relative w-full h-2 bg-slate-700 rounded-full overflow-hidden cursor-pointer group",
                                                onclick: {
                                                    let state = state_arc.clone();
                                                    let stream_id = stream.stream_id;
                                                    let total_frames = meta.total_frames;
                                                    move |e| {
                                                        let percent = e.data().page_coordinates().x / 1000.0; // Placeholder
                                                        let target_frame = (percent.clamp(0.0, 1.0) * total_frames as f64) as u64;
                                                        let target_ms = target_frame * 20;
                                                        let _ = state.seek_music(stream_id, target_ms);
                                                    }
                                                },
                                                div {
                                                    class: "h-full bg-emerald-500 transition-all duration-300",
                                                    style: "width: {stream.progress.frames_played * 100 / meta.total_frames.max(1)}%",
                                                }
                                                div {
                                                    class: "absolute top-0 left-0 w-full h-full opacity-0 group-hover:opacity-20 bg-white transition-opacity",
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
                                                        let current_ms = stream.progress.frames_played * 20;
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
                                                        let current_ms = stream.progress.frames_played * 20;
                                                        move |_| {
                                                            let _ = state.seek_music(stream_id, current_ms + 10_000);
                                                        }
                                                    },
                                                    "⏩"
                                                }
                                            }

                                            p {
                                                class: "text-xs text-slate-500 mt-2 text-center",
                                                "{stream.progress.frames_played * 20 / 1000}s / {meta.total_frames * 20 / 1000}s"
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
