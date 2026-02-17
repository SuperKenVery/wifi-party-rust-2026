//! Main application entry point for the UI.

use crate::state::{AppState, HostInfo};
use dioxus::prelude::*;
use std::sync::Arc;

use super::sidebar::SidebarMenu;
use super::sidebar_panels::{AudioControlPanel, DebugPanel, ParticipantsPanel, ShareMusicPanel};
use crate::party::{NtpDebugInfo, SyncedStreamState};

const NARROW_BREAKPOINT: u32 = 600;

#[derive(Clone, Copy)]
pub struct UIState {
    pub active_hosts: Signal<Vec<HostInfo>>,
    pub mic_volume: Signal<f32>,
    pub mic_audio_level: Signal<u32>,
    pub loopback_enabled: Signal<bool>,
    pub system_audio_enabled: Signal<bool>,
    pub system_audio_level: Signal<u32>,
    pub listen_enabled: Signal<bool>,
    pub ntp_info: Signal<Option<NtpDebugInfo>>,
    pub synced_streams: Signal<Vec<SyncedStreamState>>,
    pub is_narrow: Signal<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq, Routable)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AppLayout)]
        #[route("/")]
        Menu,
        #[route("/senders")]
        Senders,
        #[route("/audio")]
        AudioControl,
        #[route("/music")]
        ShareMusic,
        #[route("/debug")]
        Debug,
}

impl Route {
    pub fn label(&self) -> &'static str {
        match self {
            Route::Menu => "Menu",
            Route::Senders => "Senders",
            Route::AudioControl => "Audio Control",
            Route::ShareMusic => "Share Music",
            Route::Debug => "Debug",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Route::Menu => "",
            Route::Senders => "👥",
            Route::AudioControl => "🎛️",
            Route::ShareMusic => "🎵",
            Route::Debug => "🔧",
        }
    }

    pub fn menu_items() -> [Route; 4] {
        [Route::Senders, Route::AudioControl, Route::ShareMusic, Route::Debug]
    }
}

#[allow(non_snake_case)]
pub fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/custom.css") }
        document::Stylesheet { href: asset!("/assets/tailwind_output.css") }

        Router::<Route> {}
    }
}

#[allow(non_snake_case)]
fn AppLayout() -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    let mut ui = UIState {
        active_hosts: use_signal(Vec::<HostInfo>::new),
        mic_volume: use_signal(|| 1.0f32),
        mic_audio_level: use_signal(|| 0u32),
        loopback_enabled: use_signal(|| false),
        system_audio_enabled: use_signal(|| false),
        system_audio_level: use_signal(|| 0u32),
        listen_enabled: use_signal(|| true),
        ntp_info: use_signal(|| None::<NtpDebugInfo>),
        synced_streams: use_signal(Vec::<SyncedStreamState>::new),
        is_narrow: use_signal(|| false),
    };

    use_effect(move || {
        spawn(async move {
            let mut eval = document::eval(
                r#"
                function checkWidth() {
                    dioxus.send(window.innerWidth);
                }
                checkWidth();
                window.addEventListener('resize', checkWidth);
                "#,
            );
            loop {
                if let Ok(width) = eval.recv::<u32>().await {
                    let narrow = width < NARROW_BREAKPOINT;
                    if (ui.is_narrow)() != narrow {
                        ui.is_narrow.set(narrow);
                    }
                }
            }
        });
    });

    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            loop {
                if let Ok(infos) = state.host_infos.lock() {
                    ui.active_hosts.set(infos.clone());
                }

                if let Ok(vol) = state.mic_volume.lock() {
                    ui.mic_volume.set(*vol);
                }

                let level = state
                    .mic_audio_level
                    .load(std::sync::atomic::Ordering::Relaxed);
                ui.mic_audio_level.set(level);

                ui.loopback_enabled.set(
                    state
                        .loopback_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                ui.system_audio_enabled.set(
                    state
                        .system_audio_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                let sys_level = state
                    .system_audio_level
                    .load(std::sync::atomic::Ordering::Relaxed);
                ui.system_audio_level.set(sys_level);

                ui.listen_enabled.set(
                    state
                        .listen_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                ui.ntp_info.set(state.ntp_debug_info());

                ui.synced_streams.set(state.synced_stream_states());

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    });

    use_context_provider(|| ui);

    let route = use_route::<Route>();

    rsx! {
        div {
            class: "flex h-screen w-full bg-slate-900 text-slate-100 font-sans overflow-hidden selection:bg-indigo-500 selection:text-white",

            if (ui.is_narrow)() {
                Outlet::<Route> {}
            } else {
                SidebarMenu {
                    selected: Some(route),
                    full_width: false,
                }
                Outlet::<Route> {}
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn Menu() -> Element {
    let ui = use_context::<UIState>();

    if (ui.is_narrow)() {
        rsx! {
            SidebarMenu {
                selected: None,
                full_width: true,
            }
        }
    } else {
        rsx! {
            ParticipantsPanel { hosts: (ui.active_hosts)() }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn Senders() -> Element {
    let ui = use_context::<UIState>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if (ui.is_narrow)() {
        rsx! {
            ParticipantsPanel { hosts: (ui.active_hosts)(), on_back }
        }
    } else {
        rsx! {
            ParticipantsPanel { hosts: (ui.active_hosts)() }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn AudioControl() -> Element {
    let ui = use_context::<UIState>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if (ui.is_narrow)() {
        rsx! {
            AudioControlPanel {
                mic_volume: (ui.mic_volume)(),
                mic_audio_level: (ui.mic_audio_level)(),
                loopback_enabled: (ui.loopback_enabled)(),
                system_audio_enabled: (ui.system_audio_enabled)(),
                system_audio_level: (ui.system_audio_level)(),
                listen_enabled: (ui.listen_enabled)(),
                on_back,
            }
        }
    } else {
        rsx! {
            AudioControlPanel {
                mic_volume: (ui.mic_volume)(),
                mic_audio_level: (ui.mic_audio_level)(),
                loopback_enabled: (ui.loopback_enabled)(),
                system_audio_enabled: (ui.system_audio_enabled)(),
                system_audio_level: (ui.system_audio_level)(),
                listen_enabled: (ui.listen_enabled)(),
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn ShareMusic() -> Element {
    let ui = use_context::<UIState>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if (ui.is_narrow)() {
        rsx! {
            ShareMusicPanel { active_streams: (ui.synced_streams)(), on_back }
        }
    } else {
        rsx! {
            ShareMusicPanel { active_streams: (ui.synced_streams)() }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn Debug() -> Element {
    let ui = use_context::<UIState>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if (ui.is_narrow)() {
        rsx! {
            DebugPanel { ntp_info: (ui.ntp_info)(), on_back }
        }
    } else {
        rsx! {
            DebugPanel { ntp_info: (ui.ntp_info)() }
        }
    }
}
