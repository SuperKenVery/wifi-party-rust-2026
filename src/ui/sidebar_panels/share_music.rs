use crate::music_provider::{MusicProvider, MusicProviderContext};
use crate::party::{PlaylistState, SyncedStreamState};
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
    playlist: PlaylistState,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let mut selected_provider = use_signal(|| 0usize);

    // Build the provider context with two explicit actions.
    // Providers call play_now() or queue() — they don't know about AppState.
    let provider_ctx = MusicProviderContext::new(
        {
            let state = state_arc.clone();
            move |data, title| state.start_music_stream(data, title)
        },
        {
            let state = state_arc.clone();
            move |data, title| state.playlist_add(data, title)
        },
    );

    let providers: Vec<Box<dyn MusicProvider>> = state_arc
        .music_provider_factories
        .iter()
        .map(|f| f(provider_ctx.clone()))
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
    let local_sender_stream_id = active_streams
        .iter()
        .find(|stream| stream.is_local_sender)
        .map(|stream| stream.stream_id);
    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",

            PanelHeader { title: "Share Music", on_back }

            div {
                class: "flex-1 overflow-y-auto p-8 pt-0",

                div {
                    class: "max-w-2xl space-y-6",

                    div {
                        class: "glass-card p-6 rounded-2xl space-y-6",

                        p {
                            class: "text-sm text-slate-400",
                            "Share local files or Apple Music tracks with all participants. Playback stays synchronized across connected devices using shared party time."
                        }

                        div {
                            class: "flex items-center gap-3",
                            label {
                                class: "relative inline-flex items-center cursor-pointer",
                                input {
                                    r#type: "checkbox",
                                    class: "sr-only peer",
                                    checked: state_arc.vocal_removal_enabled.load(std::sync::atomic::Ordering::Relaxed),
                                    onchange: {
                                        let state = state_arc.clone();
                                        let stream_id = local_sender_stream_id;
                                        move |evt: Event<FormData>| {
                                            let checked = evt.checked();
                                            if let Some(stream_id) = stream_id {
                                                let _ = state.set_music_vocal_removal(stream_id, checked);
                                            } else {
                                                state.vocal_removal_enabled.store(checked, std::sync::atomic::Ordering::Relaxed);
                                            }
                                        }
                                    },
                                }
                                div {
                                    class: "w-9 h-5 bg-slate-700 rounded-full peer peer-checked:bg-pink-500 after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-full",
                                }
                            }
                            span {
                                class: "text-sm text-slate-400 font-medium",
                                "Remove vocal"
                            }
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

                    // Shared Playlist section
                    PlaylistSection {
                        playlist: playlist.clone(),
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

// ---------------------------------------------------------------------------
//  Shared Playlist UI
// ---------------------------------------------------------------------------

#[allow(non_snake_case)]
#[component]
fn PlaylistSection(playlist: PlaylistState) -> Element {
    let state = use_context::<Arc<AppState>>();
    let entries = playlist.entries.clone();
    let current_id = playlist.current_entry_id;
    let is_empty = entries.is_empty();

    rsx! {
        div {
            class: "glass-card p-6 rounded-2xl space-y-4",

            div {
                class: "flex items-center justify-between",
                div {
                    class: "flex items-center gap-2",
                    span { class: "text-xl", "📋" }
                    h3 { class: "text-lg font-bold text-white", "Shared Playlist" }
                    if !is_empty {
                        span {
                            class: "px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 text-xs font-bold border border-indigo-500/30",
                            "{entries.len()}"
                        }
                    }
                }
                if !is_empty {
                    button {
                        class: "text-xs text-slate-400 hover:text-rose-400 transition-colors",
                        onclick: {
                            let state = state.clone();
                            move |_| {
                                let _ = state.playlist_clear();
                            }
                        },
                        "Clear all"
                    }
                }
            }

            if is_empty {
                p {
                    class: "text-sm text-slate-500 italic",
                    "No songs in the playlist yet. Switch to \"Add to Playlist\" mode above and pick a song to queue it here for everyone."
                }
            } else {
                // Transport controls (skip / previous)
                div {
                    class: "flex items-center justify-center gap-4 py-2",
                    button {
                        class: "p-2 rounded-full hover:bg-indigo-500/20 text-indigo-400 transition-colors disabled:opacity-30 disabled:cursor-not-allowed",
                        disabled: current_id.is_none(),
                        onclick: {
                            let state = state.clone();
                            move |_| {
                                let _ = state.playlist_previous();
                            }
                        },
                        "⏮"
                    }
                    button {
                        class: "p-3 rounded-full bg-indigo-500/20 hover:bg-indigo-500/30 text-indigo-400 transition-colors disabled:opacity-30 disabled:cursor-not-allowed",
                        disabled: current_id.is_none(),
                        onclick: {
                            let state = state.clone();
                            move |_| {
                                let _ = state.playlist_skip();
                            }
                        },
                        "⏭"
                    }
                }

                // Entry list
                div {
                    class: "space-y-1",
                    for (idx, entry) in entries.iter().enumerate() {
                        {
                            let entry = entry.clone();
                            let is_current = current_id == Some(entry.entry_id);
                            let entry_id_play = entry.entry_id;
                            let entry_id_remove = entry.entry_id;
                            let entry_id_up = entry.entry_id;
                            let entry_id_down = entry.entry_id;
                            let up_target = idx.saturating_sub(1);
                            let down_target = (idx + 1).min(entries.len().saturating_sub(1));
                            let can_move_up = idx > 0;
                            let can_move_down = idx + 1 < entries.len();
                            rsx! {
                                div {
                                    key: "{entry.entry_id}",
                                    class: if is_current {
                                        "flex items-center gap-3 p-3 rounded-xl bg-indigo-500/15 border border-indigo-500/40"
                                    } else {
                                        "flex items-center gap-3 p-3 rounded-xl bg-slate-800/60 hover:bg-slate-700/80 border border-transparent hover:border-slate-600 transition-colors"
                                    },
                                    // Index / current indicator
                                    div {
                                        class: if is_current {
                                            "shrink-0 w-7 h-7 rounded-full bg-indigo-500/30 flex items-center justify-center text-indigo-300 text-xs font-bold"
                                        } else {
                                            "shrink-0 w-7 h-7 rounded-full bg-slate-700 flex items-center justify-center text-slate-400 text-xs font-bold"
                                        },
                                        if is_current { "▶" } else { "{idx + 1}" }
                                    }
                                    // Title + added_by
                                    div {
                                        class: "flex-1 min-w-0 cursor-pointer",
                                        onclick: {
                                            let state = state.clone();
                                            move |_| {
                                                let _ = state.playlist_play(entry_id_play);
                                            }
                                        },
                                        p {
                                            class: if is_current {
                                                "text-sm font-medium text-indigo-300 truncate"
                                            } else {
                                                "text-sm font-medium text-white truncate"
                                            },
                                            "{entry.title}"
                                        }
                                        p {
                                            class: "text-xs text-slate-500 truncate",
                                            "added by {entry.added_by}"
                                        }
                                    }
                                    // Move up
                                    button {
                                        class: "shrink-0 w-7 h-7 rounded-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-slate-700 transition-colors disabled:opacity-20 disabled:cursor-not-allowed",
                                        disabled: !can_move_up,
                                        onclick: {
                                            let state = state.clone();
                                            move |_| {
                                                let _ = state.playlist_move(entry_id_up, up_target);
                                            }
                                        },
                                        "↑"
                                    }
                                    // Move down
                                    button {
                                        class: "shrink-0 w-7 h-7 rounded-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-slate-700 transition-colors disabled:opacity-20 disabled:cursor-not-allowed",
                                        disabled: !can_move_down,
                                        onclick: {
                                            let state = state.clone();
                                            move |_| {
                                                let _ = state.playlist_move(entry_id_down, down_target);
                                            }
                                        },
                                        "↓"
                                    }
                                    // Remove
                                    button {
                                        class: "shrink-0 w-7 h-7 rounded-full flex items-center justify-center text-slate-400 hover:text-rose-400 hover:bg-rose-500/10 transition-colors",
                                        onclick: {
                                            let state = state.clone();
                                            move |_| {
                                                let _ = state.playlist_remove(entry_id_remove);
                                            }
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
