mod audio_control;
mod debug;
mod participants;
mod share_music;

pub use audio_control::AudioControlPanel;
pub use debug::DebugPanel;
pub use participants::MainContent as ParticipantsPanel;
pub use share_music::ShareMusicPanel;

use dioxus::prelude::*;

#[allow(non_snake_case)]
#[component]
pub fn PanelHeader(
    title: &'static str,
    #[props(default)] badge: Option<String>,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div {
            class: "h-20 px-8 flex items-center justify-between z-10",
            div {
                class: "flex items-center gap-4",
                if let Some(handler) = on_back {
                    button {
                        class: "w-10 h-10 -ml-2 rounded-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-slate-800 transition-colors",
                        onclick: move |_| handler.call(()),
                        "←"
                    }
                }
                h2 { class: "text-xl font-bold text-white", "{title}" }
                if let Some(badge_text) = badge {
                    span {
                        class: "px-2.5 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 text-xs font-bold border border-indigo-500/30",
                        "{badge_text}"
                    }
                }
            }
        }
    }
}
