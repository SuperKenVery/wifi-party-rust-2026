//! Main application entry point for the UI.

use crate::state::{AppState, HostInfo};
use dioxus::prelude::*;
use std::sync::Arc;

use super::sidebar::{MenuSection, SidebarMenu};
use super::sidebar_panels::{AudioControlPanel, DebugPanel, ParticipantsPanel, ShareMusicPanel};
use crate::party::{NtpDebugInfo, SyncedStreamState};

const NARROW_BREAKPOINT: u32 = 600;

#[allow(non_snake_case)]
pub fn App() -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    let mut active_hosts = use_signal(Vec::<HostInfo>::new);
    let mut mic_volume = use_signal(|| 1.0f32);
    let mut mic_audio_level = use_signal(|| 0u32);
    let mut loopback_enabled = use_signal(|| false);
    let mut system_audio_enabled = use_signal(|| false);
    let mut system_audio_level = use_signal(|| 0u32);
    let mut listen_enabled = use_signal(|| true);
    let mut selected_section = use_signal(|| MenuSection::Senders);
    let mut ntp_info = use_signal(|| None::<NtpDebugInfo>);
    let mut synced_streams = use_signal(Vec::<SyncedStreamState>::new);

    let mut is_narrow = use_signal(|| false);
    let mut show_panel = use_signal(|| false);

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
                        if !narrow {
                            show_panel.set(false);
                        }
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

    let on_section_select = move |section: MenuSection| {
        selected_section.set(section);
        if is_narrow() {
            show_panel.set(true);
        }
    };

    let on_back = move |_| {
        show_panel.set(false);
    };

    let render_panel = move || {
        if is_narrow() {
            match selected_section() {
                MenuSection::Senders => rsx! {
                    ParticipantsPanel { hosts: active_hosts(), on_back }
                },
                MenuSection::AudioControl => rsx! {
                    AudioControlPanel {
                        mic_volume: mic_volume(),
                        mic_audio_level: mic_audio_level(),
                        loopback_enabled: loopback_enabled(),
                        system_audio_enabled: system_audio_enabled(),
                        system_audio_level: system_audio_level(),
                        listen_enabled: listen_enabled(),
                        on_back,
                    }
                },
                MenuSection::ShareMusic => rsx! {
                    ShareMusicPanel { active_streams: synced_streams(), on_back }
                },
                MenuSection::Debug => rsx! {
                    DebugPanel { ntp_info: ntp_info(), on_back }
                },
            }
        } else {
            match selected_section() {
                MenuSection::Senders => rsx! {
                    ParticipantsPanel { hosts: active_hosts() }
                },
                MenuSection::AudioControl => rsx! {
                    AudioControlPanel {
                        mic_volume: mic_volume(),
                        mic_audio_level: mic_audio_level(),
                        loopback_enabled: loopback_enabled(),
                        system_audio_enabled: system_audio_enabled(),
                        system_audio_level: system_audio_level(),
                        listen_enabled: listen_enabled(),
                    }
                },
                MenuSection::ShareMusic => rsx! {
                    ShareMusicPanel { active_streams: synced_streams() }
                },
                MenuSection::Debug => rsx! {
                    DebugPanel { ntp_info: ntp_info() }
                },
            }
        }
    };

    rsx! {
        document::Stylesheet { href: asset!("/assets/custom.css") }
        document::Stylesheet { href: asset!("/assets/tailwind_output.css") }

        div {
            class: "flex h-screen w-full bg-slate-900 text-slate-100 font-sans overflow-hidden selection:bg-indigo-500 selection:text-white",

            if is_narrow() {
                if show_panel() {
                    {render_panel()}
                } else {
                    SidebarMenu {
                        selected: selected_section(),
                        on_select: on_section_select,
                        full_width: true,
                    }
                }
            } else {
                SidebarMenu {
                    selected: selected_section(),
                    on_select: on_section_select,
                    full_width: false,
                }
                {render_panel()}
            }
        }
    }
}
