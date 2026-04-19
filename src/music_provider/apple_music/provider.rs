//! Dioxus UI wiring for the Apple Music provider.
//!
//! This is intentionally minimal — the download pipeline is exercised via
//! unit tests. The UI here exists so the provider shows up in the sidebar
//! and the user can paste an Apple Music URL to start a synced stream.

use dioxus::prelude::*;
use std::sync::Arc;
use tracing::{error, info};

use crate::music_provider::MusicProvider;
use crate::state::AppState;

use super::download::{self, DEFAULT_WRAPPER_ADDR};

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
    let mut url = use_signal(String::new);
    let mut status = use_signal(|| "Paste an Apple Music song URL".to_string());
    let mut busy = use_signal(|| false);

    let on_download = move |_| {
        if *busy.read() {
            return;
        }
        let u = url.read().trim().to_string();
        if u.is_empty() {
            status.set("URL is empty".to_string());
            return;
        }
        let state = state.clone();
        busy.set(true);
        status.set("Fetching token…".to_string());
        spawn(async move {
            let client = match reqwest::Client::builder().build() {
                Ok(c) => c,
                Err(e) => {
                    status.set(format!("http client: {e}"));
                    busy.set(false);
                    return;
                }
            };
            let token = match super::api::get_token(&client).await {
                Ok(t) => t,
                Err(e) => {
                    status.set(format!("token: {e}"));
                    busy.set(false);
                    return;
                }
            };
            status.set("Downloading + decrypting…".to_string());
            let song =
                match download::download_song(&client, &token, &u, DEFAULT_WRAPPER_ADDR).await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("apple music download failed: {e:#}");
                        status.set(format!("failed: {e}"));
                        busy.set(false);
                        return;
                    }
                };
            info!(
                "apple music: got {} bytes ({})",
                song.bytes.len(),
                song.file_name
            );
            let name = song.file_name.clone();
            if let Err(e) = state.start_music_stream(song.bytes, name.clone()) {
                error!("start_music_stream: {e:#}");
                status.set(format!("play: {e}"));
            } else {
                status.set(format!("Playing {name}"));
            }
            busy.set(false);
        });
    };

    rsx! {
        div {
            class: "space-y-4",
            div {
                class: "text-sm text-gray-400",
                "{status.read()}"
            }
            input {
                class: "w-full p-3 rounded-xl bg-white/10 border border-white/20 text-white placeholder-gray-500",
                r#type: "text",
                placeholder: "https://music.apple.com/cn/album/…?i=…",
                value: "{url.read()}",
                oninput: move |evt| url.set(evt.value()),
            }
            button {
                class: "w-full p-4 rounded-2xl flex items-center justify-center gap-3 transition-all duration-200 border bg-pink-500/10 border-pink-500/50 text-pink-400 hover:bg-pink-500/20 cursor-pointer disabled:opacity-50",
                disabled: *busy.read(),
                onclick: on_download,
                div { class: "text-2xl", "🎧" }
                span { class: "text-base font-bold",
                    if *busy.read() { "Working…" } else { "Download & Play" }
                }
            }
        }
    }
}
