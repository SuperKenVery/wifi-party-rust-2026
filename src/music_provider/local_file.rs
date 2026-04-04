use dioxus::prelude::*;
use std::sync::Arc;
use tracing::error;

use crate::state::AppState;
use crate::music_provider::MusicProvider;

pub fn factory(state: Arc<AppState>) -> Box<dyn MusicProvider> {
    Box::new(LocalFileProvider::new(state))
}

pub struct LocalFileProvider {
    state: Arc<AppState>,
}

impl LocalFileProvider {
    fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl MusicProvider for LocalFileProvider {
    fn name(&self) -> &'static str {
        "Local File"
    }

    fn render(&self) -> Element {
        local_file_content(self.state.clone())
    }
}

fn local_file_content(state: Arc<AppState>) -> Element {
    rsx! {
        div {
            class: "space-y-6",
            { file_select_button(state.clone()) }
        }
    }
}

const FILE_SELECT_BUTTON_CLASS: &str = "w-full p-6 rounded-2xl flex items-center justify-center gap-4 transition-all duration-200 border bg-pink-500/10 border-pink-500/50 text-pink-400 hover:bg-pink-500/20 cursor-pointer";

fn file_select_button(state: Arc<AppState>) -> Element {
    #[cfg(target_os = "android")]
    {
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

        return rsx! {
            button {
                class: FILE_SELECT_BUTTON_CLASS,
                onclick: on_click,
                div { class: "text-3xl", "🎵" }
                span { class: "text-lg font-bold", "Select Music File" }
            }
        };
    }

    #[cfg(not(target_os = "android"))]
    {
        let on_change = move |evt: Event<FormData>| {
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
        };

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
}
