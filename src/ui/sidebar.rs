//! Sidebar menu and content panel components.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuSection {
    Senders,
    AudioControl,
    ShareMusic,
    Debug,
}

impl MenuSection {
    pub fn label(&self) -> &'static str {
        match self {
            MenuSection::Senders => "Senders",
            MenuSection::AudioControl => "Audio Control",
            MenuSection::ShareMusic => "Share Music",
            MenuSection::Debug => "Debug",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            MenuSection::Senders => "👥",
            MenuSection::AudioControl => "🎛️",
            MenuSection::ShareMusic => "🎵",
            MenuSection::Debug => "🔧",
        }
    }
}

#[allow(non_snake_case)]
#[component]
pub fn SidebarMenu(selected: MenuSection, on_select: EventHandler<MenuSection>) -> Element {
    rsx! {
        div {
            class: "w-56 flex-shrink-0 flex flex-col glass-strong border-r border-slate-800 z-20",

            div {
                class: "p-6 pb-4",
                div {
                    class: "flex items-center gap-3 mb-1",
                    span { class: "text-2xl", "🎤" }
                    h1 {
                        class: "text-xl font-bold tracking-tight gradient-text-hero",
                        "Wi-Fi Party"
                    }
                }
                p {
                    class: "text-[10px] text-slate-400 font-medium ml-1 uppercase tracking-wider",
                    "Local Audio Sharing"
                }
            }

            div {
                class: "flex-1 px-3 py-4 space-y-1",

                for section in [MenuSection::Senders, MenuSection::AudioControl, MenuSection::ShareMusic, MenuSection::Debug] {
                    MenuItem {
                        section,
                        is_selected: selected == section,
                        on_click: move |_| on_select.call(section),
                    }
                }
            }

            div {
                class: "p-4 border-t border-slate-800/50 text-center text-[10px] text-slate-500",
                "v0.1.0 • UDP Multicast"
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn MenuItem(section: MenuSection, is_selected: bool, on_click: EventHandler<()>) -> Element {
    let base_class =
        "flex items-center gap-3 px-4 py-3 rounded-xl cursor-pointer transition-all duration-200";
    let selected_class = if is_selected {
        "bg-indigo-500/20 text-indigo-300 border border-indigo-500/30"
    } else {
        "text-slate-400 hover:bg-slate-800 hover:text-slate-200 border border-transparent"
    };

    rsx! {
        div {
            class: "{base_class} {selected_class}",
            onclick: move |_| on_click.call(()),
            span { class: "text-lg", "{section.icon()}" }
            span { class: "text-sm font-medium", "{section.label()}" }
        }
    }
}
