//! Main application entry point for the UI.

use crate::state::{AppState, HostInfo};
use dioxus::prelude::*;
use std::sync::Arc;

use super::sidebar::SidebarMenu;
use super::sidebar_panels::{AudioControlPanel, DebugPanel, ParticipantsPanel, ShareMusicPanel};
use crate::party::{NtpDebugInfo, SyncedStreamState};

const NARROW_BREAKPOINT: u32 = 600;

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

    let mut active_hosts = use_signal(Vec::<HostInfo>::new);
    let mut mic_volume = use_signal(|| 1.0f32);
    let mut mic_audio_level = use_signal(|| 0u32);
    let mut loopback_enabled = use_signal(|| false);
    let mut system_audio_enabled = use_signal(|| false);
    let mut system_audio_level = use_signal(|| 0u32);
    let mut listen_enabled = use_signal(|| true);
    let mut ntp_info = use_signal(|| None::<NtpDebugInfo>);
    let mut synced_streams = use_signal(Vec::<SyncedStreamState>::new);

    let mut is_narrow = use_signal(|| false);

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
                    if is_narrow() != narrow {
                        is_narrow.set(narrow);
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
                    active_hosts.set(infos.clone());
                }

                if let Ok(vol) = state.mic_volume.lock() {
                    mic_volume.set(*vol);
                }

                let level = state
                    .mic_audio_level
                    .load(std::sync::atomic::Ordering::Relaxed);
                mic_audio_level.set(level);

                loopback_enabled.set(
                    state
                        .loopback_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                system_audio_enabled.set(
                    state
                        .system_audio_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                let sys_level = state
                    .system_audio_level
                    .load(std::sync::atomic::Ordering::Relaxed);
                system_audio_level.set(sys_level);

                listen_enabled.set(
                    state
                        .listen_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                ntp_info.set(state.ntp_debug_info());

                synced_streams.set(state.synced_stream_states());

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    });

    use_context_provider(|| active_hosts);
    use_context_provider(|| mic_volume);
    use_context_provider(|| mic_audio_level);
    use_context_provider(|| loopback_enabled);
    use_context_provider(|| system_audio_enabled);
    use_context_provider(|| system_audio_level);
    use_context_provider(|| listen_enabled);
    use_context_provider(|| ntp_info);
    use_context_provider(|| synced_streams);
    use_context_provider(|| is_narrow);

    let route = use_route::<Route>();

    rsx! {
        div {
            class: "flex h-screen w-full bg-slate-900 text-slate-100 font-sans overflow-hidden selection:bg-indigo-500 selection:text-white",

            if is_narrow() {
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
    let is_narrow = use_context::<Signal<bool>>();
    let active_hosts = use_context::<Signal<Vec<HostInfo>>>();

    if is_narrow() {
        rsx! {
            SidebarMenu {
                selected: None,
                full_width: true,
            }
        }
    } else {
        rsx! {
            ParticipantsPanel { hosts: active_hosts() }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn Senders() -> Element {
    let is_narrow = use_context::<Signal<bool>>();
    let active_hosts = use_context::<Signal<Vec<HostInfo>>>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if is_narrow() {
        rsx! {
            ParticipantsPanel { hosts: active_hosts(), on_back }
        }
    } else {
        rsx! {
            ParticipantsPanel { hosts: active_hosts() }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn AudioControl() -> Element {
    let is_narrow = use_context::<Signal<bool>>();
    let mic_volume = use_context::<Signal<f32>>();
    let mic_audio_level = use_context::<Signal<u32>>();
    let loopback_enabled = use_context::<Signal<bool>>();
    let system_audio_enabled = use_context::<Signal<bool>>();
    let system_audio_level = use_context::<Signal<u32>>();
    let listen_enabled = use_context::<Signal<bool>>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if is_narrow() {
        rsx! {
            AudioControlPanel {
                mic_volume: mic_volume(),
                mic_audio_level: mic_audio_level(),
                loopback_enabled: loopback_enabled(),
                system_audio_enabled: system_audio_enabled(),
                system_audio_level: system_audio_level(),
                listen_enabled: listen_enabled(),
                on_back,
            }
        }
    } else {
        rsx! {
            AudioControlPanel {
                mic_volume: mic_volume(),
                mic_audio_level: mic_audio_level(),
                loopback_enabled: loopback_enabled(),
                system_audio_enabled: system_audio_enabled(),
                system_audio_level: system_audio_level(),
                listen_enabled: listen_enabled(),
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn ShareMusic() -> Element {
    let is_narrow = use_context::<Signal<bool>>();
    let synced_streams = use_context::<Signal<Vec<SyncedStreamState>>>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if is_narrow() {
        rsx! {
            ShareMusicPanel { active_streams: synced_streams(), on_back }
        }
    } else {
        rsx! {
            ShareMusicPanel { active_streams: synced_streams() }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn Debug() -> Element {
    let is_narrow = use_context::<Signal<bool>>();
    let ntp_info = use_context::<Signal<Option<NtpDebugInfo>>>();
    let nav = use_navigator();

    let on_back = move |_| {
        nav.push(Route::Menu);
    };

    if is_narrow() {
        rsx! {
            DebugPanel { ntp_info: ntp_info(), on_back }
        }
    } else {
        rsx! {
            DebugPanel { ntp_info: ntp_info() }
        }
    }
}
