use dioxus::prelude::*;
use tracing::error;

use crate::music_provider::{MusicProvider, MusicProviderContext};

pub fn factory(ctx: MusicProviderContext) -> Box<dyn MusicProvider> {
    Box::new(LocalFileProvider::new(ctx))
}

pub struct LocalFileProvider {
    ctx: MusicProviderContext,
}

impl LocalFileProvider {
    fn new(ctx: MusicProviderContext) -> Self {
        Self { ctx }
    }
}

impl MusicProvider for LocalFileProvider {
    fn name(&self) -> &'static str {
        "Local File"
    }

    fn render(&self) -> Element {
        local_file_content(self.ctx.clone())
    }
}

fn local_file_content(ctx: MusicProviderContext) -> Element {
    rsx! {
        div {
            class: "space-y-3",
            { file_button(ctx.clone(), FileAction::PlayNow) }
            { file_button(ctx.clone(), FileAction::Queue) }
        }
    }
}

#[derive(Clone, Copy)]
enum FileAction {
    PlayNow,
    Queue,
}

impl FileAction {
    fn label(&self) -> &'static str {
        match self {
            FileAction::PlayNow => "Select & Play",
            FileAction::Queue => "Select & Queue",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            FileAction::PlayNow => "▶",
            FileAction::Queue => "📋",
        }
    }

    fn class(&self) -> &'static str {
        match self {
            FileAction::PlayNow => {
                "w-full p-4 rounded-2xl flex items-center justify-center gap-3 transition-all duration-200 border bg-pink-500/10 border-pink-500/50 text-pink-400 hover:bg-pink-500/20 cursor-pointer"
            }
            FileAction::Queue => {
                "w-full p-4 rounded-2xl flex items-center justify-center gap-3 transition-all duration-200 border bg-indigo-500/10 border-indigo-500/50 text-indigo-400 hover:bg-indigo-500/20 cursor-pointer"
            }
        }
    }
}

fn file_button(ctx: MusicProviderContext, action: FileAction) -> Element {
    #[cfg(target_os = "android")]
    {
        let on_click = {
            use crate::io::pick_audio_file;
            use tracing::info;

            move |_| {
                let ctx = ctx.clone();
                spawn(async move {
                    info!("Opening native file picker...");
                    let Some(result) = pick_audio_file().await else {
                        info!("File picker returned None (cancelled or error)");
                        return;
                    };

                    info!("Got file: {}", result.name);
                    let res = match action {
                        FileAction::PlayNow => ctx.play_now(result.data, result.name),
                        FileAction::Queue => ctx.queue(result.data, result.name),
                    };
                    if let Err(e) = res {
                        error!("Failed to submit audio: {}", e);
                    }
                });
            }
        };

        return rsx! {
            button {
                class: action.class(),
                onclick: on_click,
                span { class: "text-xl", "{action.icon()}" }
                span { class: "text-sm font-bold", "{action.label()}" }
            }
        };
    }

    #[cfg(not(target_os = "android"))]
    {
        let on_change = move |evt: Event<FormData>| {
            let ctx = ctx.clone();
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

                let res = match action {
                    FileAction::PlayNow => ctx.play_now(bytes.to_vec(), file_name),
                    FileAction::Queue => ctx.queue(bytes.to_vec(), file_name),
                };
                if let Err(e) = res {
                    error!("Failed to submit audio: {}", e);
                }
            });
        };

        rsx! {
            label {
                class: action.class(),
                span { class: "text-xl", "{action.icon()}" }
                span { class: "text-sm font-bold", "{action.label()}" }
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
