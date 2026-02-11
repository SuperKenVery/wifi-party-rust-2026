use crate::party::PartyConfig;
use crate::state::AppState;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, DeviceId};
use dioxus::prelude::*;
use network_interface::NetworkInterfaceConfig;
use std::net::IpAddr;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct NetworkInterfaceInfo {
    name: String,
    index: u32,
    v4_addrs: Vec<std::net::Ipv4Addr>,
    v6_addrs: Vec<std::net::Ipv6Addr>,
}

impl NetworkInterfaceInfo {
    fn display_name(&self, ipv6: bool) -> String {
        let addrs = if ipv6 {
            self.v6_addrs
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            self.v4_addrs
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        if addrs.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, addrs)
        }
    }
}

fn get_input_devices() -> Vec<Device> {
    cpal::default_host()
        .input_devices()
        .map(|d| d.collect())
        .unwrap_or_default()
}

fn get_output_devices() -> Vec<Device> {
    cpal::default_host()
        .output_devices()
        .map(|d| d.collect())
        .unwrap_or_default()
}

fn get_network_interfaces() -> Vec<NetworkInterfaceInfo> {
    network_interface::NetworkInterface::show()
        .map(|ifaces| {
            let mut result: Vec<NetworkInterfaceInfo> = Vec::new();
            for iface in ifaces {
                let v4_addrs: Vec<_> = iface
                    .addr
                    .iter()
                    .filter_map(|a| match a.ip() {
                        IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
                        _ => None,
                    })
                    .collect();
                let v6_addrs: Vec<_> = iface
                    .addr
                    .iter()
                    .filter_map(|a| match a.ip() {
                        IpAddr::V6(ip) if !ip.is_loopback() => Some(ip),
                        _ => None,
                    })
                    .collect();
                if (!v4_addrs.is_empty() || !v6_addrs.is_empty())
                    && !result.iter().any(|r| r.index == iface.index)
                {
                    result.push(NetworkInterfaceInfo {
                        name: iface.name.clone(),
                        index: iface.index,
                        v4_addrs,
                        v6_addrs,
                    });
                }
            }
            result
        })
        .unwrap_or_default()
}

#[allow(deprecated)]
fn device_display_name(device: &Device) -> String {
    match device.description() {
        Ok(desc) => desc.name().to_string(),
        Err(_) => String::from("Unknown"),
    }
}

#[allow(non_snake_case)]
#[component]
pub fn AudioControlPanel(
    mic_enabled: bool,
    mic_volume: f32,
    mic_audio_level: u32,
    loopback_enabled: bool,
    system_audio_enabled: bool,
    system_audio_level: u32,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    let state_mic = state_arc.clone();
    let on_mic_toggle = move |_| {
        let current = state_mic
            .mic_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        state_mic
            .mic_enabled
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let state_vol = state_arc.clone();
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>()
            && let Ok(mut vol) = state_vol.mic_volume.lock()
        {
            *vol = value_str / 100.0;
        }
    };

    let state_loop = state_arc.clone();
    let on_loopback_toggle = move |_| {
        let current = state_loop
            .loopback_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        state_loop
            .loopback_enabled
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let state_sys = state_arc.clone();
    let on_system_audio_toggle = move |_| {
        let current = state_sys
            .system_audio_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        state_sys
            .system_audio_enabled
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",

            div {
                class: "h-20 px-8 flex items-center justify-between z-10",
                div {
                    class: "flex items-center gap-4",
                    h2 { class: "text-xl font-bold text-white", "Audio Control" }
                }
            }

            div {
                class: "flex-1 overflow-y-auto p-8 pt-0",

                div {
                    class: "max-w-2xl space-y-8",

                    div {
                        class: "glass-card p-6 rounded-2xl",

                        div {
                            class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-6",
                            "Audio Settings"
                        }

                        div {
                            class: "grid grid-cols-3 gap-4 mb-8",

                            button {
                                class: format!(
                                    "p-4 rounded-xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}",
                                    if mic_enabled { "bg-emerald-500/10 border-emerald-500/50 text-emerald-400 hover:bg-emerald-500/20" }
                                    else { "bg-rose-500/10 border-rose-500/50 text-rose-400 hover:bg-rose-500/20" }
                                ),
                                onclick: on_mic_toggle,
                                div { class: "text-2xl", if mic_enabled { "🎙️" } else { "🔇" } }
                                span { class: "text-xs font-bold", if mic_enabled { "Mic On" } else { "Mic Off" } }
                            }

                            button {
                                class: format!(
                                    "p-4 rounded-xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}",
                                    if loopback_enabled { "bg-indigo-500/10 border-indigo-500/50 text-indigo-400 hover:bg-indigo-500/20" }
                                    else { "bg-slate-800 border-slate-700 text-slate-400 hover:bg-slate-700 hover:text-slate-300" }
                                ),
                                onclick: on_loopback_toggle,
                                div { class: "text-2xl", "🎧" }
                                span { class: "text-xs font-bold", if loopback_enabled { "Loopback" } else { "No Loop" } }
                            }

                            button {
                                class: format!(
                                    "p-4 rounded-xl flex flex-col items-center justify-center gap-2 transition-all duration-200 border {}",
                                    if system_audio_enabled { "bg-purple-500/10 border-purple-500/50 text-purple-400 hover:bg-purple-500/20" }
                                    else { "bg-slate-800 border-slate-700 text-slate-400 hover:bg-slate-700 hover:text-slate-300" }
                                ),
                                onclick: on_system_audio_toggle,
                                div { class: "text-2xl", "🔊" }
                                span { class: "text-xs font-bold", if system_audio_enabled { "Sharing" } else { "Not Share" } }
                            }
                        }

                        div {
                            class: "space-y-6",

                            div {
                                div {
                                    class: "flex justify-between text-sm mb-2",
                                    span { class: "text-slate-400", "Input Gain" }
                                    span { class: "font-mono font-bold text-slate-200", "{(mic_volume * 100.0) as i32}%" }
                                }
                                input {
                                    r#type: "range",
                                    min: 0,
                                    max: 200,
                                    value: (mic_volume * 100.0) as i32,
                                    class: "w-full",
                                    oninput: on_volume_change,
                                }
                            }

                            div {
                                div {
                                    class: "flex justify-between text-sm mb-2",
                                    span { class: "text-slate-400", "Mic Level" }
                                }
                                div {
                                    class: "h-3 bg-slate-800 rounded-full overflow-hidden relative",
                                    div {
                                        class: "absolute inset-0",
                                        style: "background: linear-gradient(to right, #22c55e 0%, #22c55e 50%, #eab308 75%, #ef4444 100%)",
                                    }
                                    div {
                                        class: "absolute inset-0 bg-slate-800 transition-all duration-75",
                                        style: "left: {mic_audio_level}%",
                                    }
                                }
                            }

                            div {
                                div {
                                    class: "flex justify-between text-sm mb-2",
                                    span { class: "text-slate-400", "System Audio Level" }
                                }
                                div {
                                    class: "h-3 bg-slate-800 rounded-full overflow-hidden relative",
                                    div {
                                        class: "absolute inset-0",
                                        style: "background: linear-gradient(to right, #22c55e 0%, #22c55e 50%, #eab308 75%, #ef4444 100%)",
                                    }
                                    div {
                                        class: "absolute inset-0 bg-slate-800 transition-all duration-75",
                                        style: "left: {system_audio_level}%",
                                    }
                                }
                            }
                        }
                    }

                    DeviceSettings {}
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn DeviceSelector(
    label: &'static str,
    options: Vec<(String, String)>,
    selected: String,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            label {
                class: "block text-sm text-slate-400 mb-2",
                "{label}"
            }
            select {
                class: "w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-3 text-sm text-slate-200 focus:outline-none focus:border-indigo-500 transition-colors",
                value: "{selected}",
                onchange: move |evt| on_change.call(evt.value()),
                for (value, display) in options.iter() {
                    option {
                        value: "{value}",
                        selected: *value == selected,
                        "{display}"
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn DeviceSettings() -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    let input_devices = use_signal(get_input_devices);
    let output_devices = use_signal(get_output_devices);
    let network_interfaces = use_signal(get_network_interfaces);

    let mut selected_input = use_signal(String::new);
    let mut selected_output = use_signal(String::new);
    let mut selected_interface = use_signal(String::new);
    let mut use_ipv6 = use_signal(|| false);

    let input_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "System Default".to_string()))
            .chain(input_devices.read().iter().filter_map(|d| {
                d.id().ok().map(|id| {
                    let id_str = format!("{:?}", id);
                    (id_str, device_display_name(d))
                })
            }))
            .collect();

    let output_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "System Default".to_string()))
            .chain(output_devices.read().iter().filter_map(|d| {
                d.id().ok().map(|id| {
                    let id_str = format!("{:?}", id);
                    (id_str, device_display_name(d))
                })
            }))
            .collect();

    let ipv6 = *use_ipv6.read();
    let interface_options: Vec<(String, String)> =
        std::iter::once(("".to_string(), "System Default".to_string()))
            .chain(network_interfaces.read().iter().filter_map(|iface| {
                let supported = if ipv6 {
                    !iface.v6_addrs.is_empty()
                } else {
                    !iface.v4_addrs.is_empty()
                };
                if supported {
                    Some((iface.index.to_string(), iface.display_name(ipv6)))
                } else {
                    None
                }
            }))
            .collect();

    let on_apply = {
        let state = state_arc.clone();
        move |_| {
            let input_id: Option<DeviceId> = {
                let sel = selected_input.read();
                if sel.is_empty() {
                    None
                } else {
                    input_devices
                        .read()
                        .iter()
                        .find(|d| d.id().ok().map(|id| format!("{:?}", id)) == Some(sel.clone()))
                        .and_then(|d| d.id().ok())
                }
            };

            let output_id: Option<DeviceId> = {
                let sel = selected_output.read();
                if sel.is_empty() {
                    None
                } else {
                    output_devices
                        .read()
                        .iter()
                        .find(|d| d.id().ok().map(|id| format!("{:?}", id)) == Some(sel.clone()))
                        .and_then(|d| d.id().ok())
                }
            };

            let send_interface_index: Option<u32> = {
                let sel = selected_interface.read();
                if sel.is_empty() {
                    None
                } else {
                    sel.parse().ok()
                }
            };

            let config = PartyConfig {
                input_device_id: input_id,
                output_device_id: output_id,
                ipv6: *use_ipv6.read(),
                send_interface_index,
            };

            if let Ok(mut party_guard) = state.party.lock()
                && let Some(party) = party_guard.as_mut()
                && let Err(e) = party.restart_with_config(config)
            {
                tracing::error!("Failed to restart party: {}", e);
            }
        }
    };

    rsx! {
        div {
            class: "glass-card p-6 rounded-2xl",

            div {
                class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-6",
                "Device Settings"
            }

            div {
                class: "space-y-4",

                DeviceSelector {
                    label: "Input Device",
                    options: input_options,
                    selected: selected_input(),
                    on_change: move |v| selected_input.set(v),
                }

                DeviceSelector {
                    label: "Output Device",
                    options: output_options,
                    selected: selected_output(),
                    on_change: move |v| selected_output.set(v),
                }

                div {
                    class: "flex items-center gap-3 py-2",
                    input {
                        r#type: "checkbox",
                        id: "ipv6-toggle",
                        class: "w-4 h-4 rounded border-slate-600 bg-slate-800 text-indigo-500 focus:ring-indigo-500 focus:ring-offset-slate-900",
                        checked: *use_ipv6.read(),
                        onchange: move |evt| use_ipv6.set(evt.checked()),
                    }
                    label {
                        r#for: "ipv6-toggle",
                        class: "text-sm text-slate-300",
                        "Use IPv6 multicast"
                    }
                }

                DeviceSelector {
                    label: "Send Interface",
                    options: interface_options,
                    selected: selected_interface(),
                    on_change: move |v| selected_interface.set(v),
                }

                button {
                    class: "w-full mt-6 px-4 py-3 bg-indigo-600 hover:bg-indigo-500 text-white text-sm font-medium rounded-lg transition-colors",
                    onclick: on_apply,
                    "Apply Changes"
                }
            }
        }
    }
}
