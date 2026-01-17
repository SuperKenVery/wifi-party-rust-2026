//! Main application entry point for the UI.

use crate::state::{AppState, ConnectionStatus, HostInfo};
use dioxus::prelude::*;
use std::sync::Arc;

use super::participants::MainContent;
use super::sidebar::Sidebar;

#[allow(non_snake_case)]
pub fn App() -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    let mut connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let mut active_hosts = use_signal(|| Vec::<HostInfo>::new());
    let mut mic_enabled = use_signal(|| false);
    let mut mic_volume = use_signal(|| 1.0f32);
    let mut mic_audio_level = use_signal(|| 0u32);
    let mut loopback_enabled = use_signal(|| false);
    let mut system_audio_enabled = use_signal(|| false);
    let mut system_audio_level = use_signal(|| 0u32);

    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            loop {
                if let Ok(status) = state.connection_status.lock() {
                    connection_status.set(*status);
                }

                if let Ok(infos) = state.host_infos.lock() {
                    active_hosts.set(infos.clone());
                }

                mic_enabled.set(state.mic_enabled.load(std::sync::atomic::Ordering::Relaxed));

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

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/custom.css") }
        document::Stylesheet { href: asset!("/assets/tailwind_output.css") }
        script { src: "https://cdn.tailwindcss.com" }

        div {
            class: "flex h-screen w-full bg-slate-900 text-slate-100 font-sans overflow-hidden selection:bg-indigo-500 selection:text-white",

            Sidebar {
                connection_status: connection_status(),
                mic_enabled: mic_enabled(),
                mic_volume: mic_volume(),
                mic_audio_level: mic_audio_level(),
                loopback_enabled: loopback_enabled(),
                system_audio_enabled: system_audio_enabled(),
                system_audio_level: system_audio_level(),
            }

            MainContent {
                hosts: active_hosts(),
            }
        }
    }
}
