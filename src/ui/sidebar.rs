//! Sidebar menu and content panel components.

use dioxus::prelude::*;

use super::app::Route;

#[allow(non_snake_case)]
#[component]
pub fn SidebarMenu(
    #[props(default)] selected: Option<Route>,
    #[props(default = false)] full_width: bool,
) -> Element {
    let width_class = if full_width {
        "w-full"
    } else {
        "w-56 flex-shrink-0"
    };

    rsx! {
        div {
            class: "{width_class} flex flex-col glass-strong border-r border-slate-800 z-20",

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

                for route in Route::menu_items() {
                    MenuItem {
                        route,
                        is_selected: selected == Some(route),
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
fn MenuItem(route: Route, is_selected: bool) -> Element {
    let base_class =
        "flex items-center gap-3 px-4 py-3 rounded-xl cursor-pointer transition-all duration-200";
    let selected_class = if is_selected {
        "bg-indigo-500/20 text-indigo-300 border border-indigo-500/30"
    } else {
        "text-slate-400 hover:bg-slate-800 hover:text-slate-200 border border-transparent"
    };

    rsx! {
        Link {
            to: route,
            class: "{base_class} {selected_class}",
            span { class: "text-lg", "{route.icon()}" }
            span { class: "text-sm font-medium", "{route.label()}" }
        }
    }
}
