//! Dioxus UI wiring for the Apple Music provider.

use dioxus::prelude::*;
use std::sync::Arc;
use tracing::{error, info};

use crate::music_provider::MusicProvider;
use crate::state::AppState;

use super::api::{self, SongData};
use super::download::{self, DownloadProgress, ProgressFn};

pub fn factory(state: Arc<AppState>) -> Box<dyn MusicProvider> {
    Box::new(AppleMusicProvider::new(state))
}

pub struct AppleMusicProvider {
    state: Arc<AppState>,
}

impl AppleMusicProvider {
    fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl MusicProvider for AppleMusicProvider {
    fn name(&self) -> &'static str {
        "Apple Music"
    }

    fn render(&self) -> Element {
        apple_music_content(self.state.clone())
    }
}

fn apple_music_content(state: Arc<AppState>) -> Element {
    let mut query = use_signal(String::new);
    let mut storefront = use_signal(|| "cn".to_string());
    let mut wrapper_addr = use_signal(|| "localhost".to_string());
    let mut wrapper_port = use_signal(|| "10020".to_string());
    let mut status: Signal<Option<String>> = use_signal(|| None);
    let mut busy = use_signal(|| false);
    let mut dl_progress: Signal<Option<f32>> = use_signal(|| None);
    let mut dec_progress: Signal<Option<f32>> = use_signal(|| None);
    let mut results: Signal<Vec<SongData>> = use_signal(Vec::new);
    let mut playing_id: Signal<Option<String>> = use_signal(|| None);

    let mut do_search = move || {
        if *busy.read() {
            return;
        }
        let q = query.read().trim().to_string();
        if q.is_empty() {
            return;
        }
        let sf = storefront.read().trim().to_string();
        let sf = if sf.is_empty() { "cn".to_string() } else { sf };
        busy.set(true);
        results.set(vec![]);
        playing_id.set(None);
        status.set(Some("Searching…".to_string()));
        spawn(async move {
            let client = match reqwest::Client::builder().build() {
                Ok(c) => c,
                Err(e) => {
                    status.set(Some(format!("Error: {e}")));
                    busy.set(false);
                    return;
                }
            };
            let token = match api::get_token(&client).await {
                Ok(t) => t,
                Err(e) => {
                    status.set(Some(format!("Auth error: {e}")));
                    busy.set(false);
                    return;
                }
            };
            match api::search(&client, &token, &sf, &q, "songs", 10, 0).await {
                Ok(resp) => {
                    let songs = resp.results.songs.map(|s| s.data).unwrap_or_default();
                    status.set(if songs.is_empty() {
                        Some("No results found".to_string())
                    } else {
                        None
                    });
                    results.set(songs);
                }
                Err(e) => {
                    status.set(Some(format!("Search failed: {e}")));
                }
            }
            busy.set(false);
        });
    };

    let on_search = move |_| do_search();

    let make_play_handler = move |song: SongData| {
        let state = state.clone();
        move |_: Event<MouseData>| {
            if *busy.read() {
                return;
            }
            let sf = storefront.read().trim().to_string();
            let sf = if sf.is_empty() { "cn".to_string() } else { sf };
            let addr = wrapper_addr.read().trim().to_string();
            let addr = if addr.is_empty() { "localhost".to_string() } else { addr };
            let port = wrapper_port.read().trim().to_string();
            let port = if port.is_empty() { "10020".to_string() } else { port };
            let wrapper = format!("{addr}:{port}");
            let song = song.clone();
            let state = state.clone();
            busy.set(true);
            dl_progress.set(Some(0.0));
            dec_progress.set(None);
            playing_id.set(Some(song.id.clone()));
            status.set(None);
            // watch channel bridges the Send+Sync callback into Dioxus signals
            let (prog_tx, mut prog_rx) =
                tokio::sync::watch::channel(DownloadProgress::Download(0.0));
            let prog_tx = Arc::new(prog_tx);
            spawn(async move {
                while prog_rx.changed().await.is_ok() {
                    match *prog_rx.borrow() {
                        DownloadProgress::Download(f) => {
                            dl_progress.set(Some(f));
                            dec_progress.set(None);
                        }
                        DownloadProgress::Decrypt(f) => {
                            dl_progress.set(Some(1.0));
                            dec_progress.set(Some(f));
                        }
                    }
                }
            });
            spawn(async move {
                let client = match reqwest::Client::builder().build() {
                    Ok(c) => c,
                    Err(e) => {
                        status.set(Some(format!("Error: {e}")));
                        busy.set(false);
                        return;
                    }
                };
                let token = match api::get_token(&client).await {
                    Ok(t) => t,
                    Err(e) => {
                        status.set(Some(format!("Auth error: {e}")));
                        busy.set(false);
                        return;
                    }
                };
                let prog_cb: ProgressFn = Arc::new(move |ev| {
                    let _ = prog_tx.send(ev);
                });
                match download::download_song_by_id(
                    &client, &token, &sf, &song.id, &wrapper, Some(prog_cb),
                )
                .await
                {
                    Ok(s) => {
                        info!("apple music: got {} bytes ({})", s.bytes.len(), s.file_name);
                        dl_progress.set(Some(1.0));
                        dec_progress.set(Some(1.0));
                        let name = s.file_name.clone();
                        if let Err(e) = state.start_music_stream(s.bytes, name.clone()) {
                            error!("start_music_stream: {e:#}");
                            status.set(Some(format!("Playback error: {e}")));
                            playing_id.set(None);
                            dl_progress.set(None);
                            dec_progress.set(None);
                        }
                    }
                    Err(e) => {
                        error!("apple music download failed: {e:#}");
                        status.set(Some(format!("Download failed: {e}")));
                        playing_id.set(None);
                        dl_progress.set(None);
                        dec_progress.set(None);
                    }
                }
                busy.set(false);
            });
        }
    };

    let songs = results.read().clone();
    let current_playing_id = playing_id.read().clone();
    let is_busy = *busy.read();

    rsx! {
        div {
            class: "space-y-4",

            // Search row
            div {
                class: "flex gap-2 items-center",
                input {
                    class: "flex-1 px-3 py-2.5 rounded-xl bg-slate-800 border border-slate-700 text-white text-sm placeholder-slate-500 focus:outline-none focus:border-pink-500/50",
                    r#type: "text",
                    placeholder: "Search Apple Music…",
                    value: "{query.read()}",
                    oninput: move |evt| query.set(evt.value()),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Enter {
                            do_search();
                        }
                    },
                }
                div {
                    class: "flex items-center gap-1.5 shrink-0",
                    span { class: "text-xs text-slate-500", "Store" }
                    input {
                        class: "w-10 px-2 py-2.5 rounded-xl bg-slate-800 border border-slate-700 text-white text-sm text-center placeholder-slate-600 focus:outline-none focus:border-pink-500/50",
                        r#type: "text",
                        placeholder: "cn",
                        value: "{storefront.read()}",
                        oninput: move |evt| storefront.set(evt.value()),
                    }
                }
                button {
                    class: "shrink-0 px-4 py-2.5 rounded-xl bg-pink-500 hover:bg-pink-400 active:bg-pink-600 text-white text-sm font-semibold transition-colors disabled:opacity-40 disabled:cursor-not-allowed",
                    disabled: is_busy,
                    onclick: on_search,
                    if is_busy && songs.is_empty() { "…" } else { "Search" }
                }
            }

            // Status
            if let Some(msg) = status.read().as_ref() {
                p { class: "text-xs text-slate-400 px-1", "{msg}" }
            }
            // Progress bars — shown while downloading/decrypting
            if playing_id.read().is_some() {
                if let Some(dl) = *dl_progress.read() {
                    ProgressBar { label: "Download", frac: dl }
                }
                if let Some(dec) = *dec_progress.read() {
                    ProgressBar { label: "Decrypt", frac: dec }
                }
            }

            // Results list — each row is its own play button
            if !songs.is_empty() {
                div {
                    class: "space-y-1",
                    for song in songs.iter() {
                        {
                            let song = song.clone();
                            let is_playing = current_playing_id.as_ref().map(|id| *id == song.id).unwrap_or(false);
                            let handler = make_play_handler(song.clone());
                            rsx! {
                                button {
                                    class: if is_playing {
                                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-xl bg-pink-500/15 border border-pink-500/40 text-left cursor-pointer transition-colors"
                                    } else {
                                        "w-full flex items-center gap-3 px-3 py-2.5 rounded-xl bg-slate-800/60 hover:bg-slate-700/80 border border-transparent hover:border-slate-600 text-left cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                                    },
                                    disabled: is_busy && !is_playing,
                                    onclick: handler,
                                    // Play / spinner icon
                                    div {
                                        class: if is_playing {
                                            "shrink-0 w-7 h-7 rounded-full bg-pink-500/30 flex items-center justify-center text-pink-400 text-xs"
                                        } else {
                                            "shrink-0 w-7 h-7 rounded-full bg-slate-700 flex items-center justify-center text-slate-400 text-xs group-hover:bg-slate-600"
                                        },
                                        if is_playing && is_busy { "…" } else { "▶" }
                                    }
                                    // Text
                                    div {
                                        class: "flex-1 min-w-0",
                                        p {
                                            class: if is_playing {
                                                "text-sm font-medium text-pink-300 truncate"
                                            } else {
                                                "text-sm font-medium text-white truncate"
                                            },
                                            "{song.attributes.name}"
                                        }
                                        p {
                                            class: "text-xs text-slate-400 truncate",
                                            "{song.attributes.artist_name}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Wrapper config — secondary, at the bottom
            div {
                class: "pt-2 border-t border-slate-800 flex items-center gap-2",
                span { class: "text-xs text-slate-500 shrink-0", "Wrapper" }
                input {
                    class: "flex-1 px-2 py-1.5 rounded-lg bg-slate-800 border border-slate-700/50 text-slate-300 text-xs placeholder-slate-600 focus:outline-none focus:border-slate-600",
                    r#type: "text",
                    placeholder: "localhost",
                    value: "{wrapper_addr.read()}",
                    oninput: move |evt| wrapper_addr.set(evt.value()),
                }
                span { class: "text-slate-600 text-xs", ":" }
                input {
                    class: "w-14 px-2 py-1.5 rounded-lg bg-slate-800 border border-slate-700/50 text-slate-300 text-xs text-center placeholder-slate-600 focus:outline-none focus:border-slate-600",
                    r#type: "text",
                    placeholder: "10020",
                    value: "{wrapper_port.read()}",
                    oninput: move |evt| wrapper_port.set(evt.value()),
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn ProgressBar(label: &'static str, frac: f32) -> Element {
    let pct = (frac * 100.0).min(100.0) as u32;
    rsx! {
        div {
            class: "space-y-1",
            div {
                class: "flex justify-between text-xs text-slate-400",
                span { "{label}" }
                span { "{pct}%" }
            }
            div {
                class: "w-full h-1.5 rounded-full bg-slate-700 overflow-hidden",
                div {
                    class: "h-1.5 bg-pink-500 rounded-full transition-all duration-150",
                    style: "width: {pct}%",
                }
            }
        }
    }
}
