use crate::party::NtpDebugInfo;
use dioxus::prelude::*;
use network_interface::NetworkInterfaceConfig;
use std::net::IpAddr;

use super::PanelHeader;

#[derive(Clone, Debug)]
struct SelfInterfaceInfo {
    name: String,
    index: u32,
    addresses: Vec<IpAddr>,
}

fn get_self_interfaces() -> Vec<SelfInterfaceInfo> {
    network_interface::NetworkInterface::show()
        .map(|interfaces| {
            interfaces
                .into_iter()
                .map(|iface| {
                    let mut addresses: Vec<IpAddr> =
                        iface.addr.iter().map(|addr| addr.ip()).collect();
                    addresses.sort_by_key(|addr| addr.to_string());

                    SelfInterfaceInfo {
                        name: iface.name,
                        index: iface.index,
                        addresses,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[allow(non_snake_case)]
#[component]
pub fn DebugPanel(
    ntp_info: Option<NtpDebugInfo>,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
    let self_interfaces = use_signal(get_self_interfaces);

    rsx! {
        div {
            class: "flex-1 flex flex-col relative overflow-hidden bg-slate-900",

            PanelHeader { title: "Debug", on_back }

            div {
                class: "flex-1 overflow-y-auto p-8 pt-0",

                div {
                    class: "max-w-2xl space-y-8",

                    div {
                        class: "glass-card p-6 rounded-2xl",

                        div {
                            class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-6",
                            "Self IP Addresses"
                        }

                        if self_interfaces.read().is_empty() {
                            div {
                                class: "text-slate-500 text-sm",
                                "No network interfaces found."
                            }
                        } else {
                            div {
                                class: "space-y-3",

                                for iface in self_interfaces.read().iter() {
                                    div {
                                        class: "rounded-lg bg-slate-800/50 border border-slate-700/50 p-3",

                                        div {
                                            class: "flex items-center justify-between gap-3 mb-2",
                                            div {
                                                class: "text-sm font-medium text-slate-200",
                                                "{iface.name}"
                                            }
                                            div {
                                                class: "text-xs font-mono text-slate-500",
                                                "#{iface.index}"
                                            }
                                        }

                                        if iface.addresses.is_empty() {
                                            div {
                                                class: "text-xs text-slate-500",
                                                "No assigned IP address"
                                            }
                                        } else {
                                            div {
                                                class: "space-y-1",

                                                for addr in iface.addresses.iter() {
                                                    div {
                                                        class: "flex items-center gap-2 text-sm",
                                                        span {
                                                            class: "w-10 text-[10px] font-bold uppercase text-slate-500",
                                                            if addr.is_ipv4() { "IPv4" } else { "IPv6" }
                                                        }
                                                        span {
                                                            class: "font-mono text-slate-300 break-all",
                                                            "{addr}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div {
                        class: "glass-card p-6 rounded-2xl",

                        div {
                            class: "text-xs font-bold text-slate-500 uppercase tracking-wider mb-6",
                            "NTP Clock Synchronization"
                        }

                        if let Some(info) = &ntp_info {
                            div {
                                class: "space-y-4",

                                // Sync status indicator
                                div {
                                    class: "flex items-center gap-3",
                                    div {
                                        class: format!(
                                            "w-3 h-3 rounded-full {}",
                                            if info.synced { "bg-emerald-500 animate-pulse" } else { "bg-amber-500" }
                                        ),
                                    }
                                    span {
                                        class: format!(
                                            "text-sm font-medium {}",
                                            if info.synced { "text-emerald-400" } else { "text-amber-400" }
                                        ),
                                        if info.synced { "Synchronized" } else { "Syncing..." }
                                    }
                                }

                                // Party clock time (prominent display)
                                div {
                                    class: "p-4 rounded-xl bg-indigo-500/10 border border-indigo-500/30",
                                    div {
                                        class: "text-xs font-bold text-indigo-400 uppercase tracking-wider mb-2",
                                        "Party Clock Time"
                                    }
                                    div {
                                        class: "text-2xl font-mono text-indigo-300",
                                        "{info.party_time_formatted}"
                                    }
                                }

                                // Detailed info grid
                                div {
                                    class: "grid grid-cols-2 gap-4",

                                    DebugInfoItem {
                                        label: "Clock Offset",
                                        value: format!("{} µs", info.offset_micros),
                                    }

                                    DebugInfoItem {
                                        label: "Raw Offset",
                                        value: format_optional_micros(info.raw_offset_micros),
                                    }

                                    DebugInfoItem {
                                        label: "Last RTT",
                                        value: format_optional_micros(info.last_rtt_micros),
                                    }

                                    DebugInfoItem {
                                        label: "Best RTT",
                                        value: format_optional_micros(info.best_rtt_micros),
                                    }

                                    DebugInfoItem {
                                        label: "Offset Samples",
                                        value: format!("{}", info.offset_sample_count),
                                    }

                                    DebugInfoItem {
                                        label: "Local Time",
                                        value: format!("{} µs", info.local_time_micros),
                                    }

                                    DebugInfoItem {
                                        label: "Party Time",
                                        value: format!("{} µs", info.party_time_micros),
                                    }

                                    DebugInfoItem {
                                        label: "Pending Requests",
                                        value: format!("{}", info.pending_requests),
                                    }

                                    DebugInfoItem {
                                        label: "Pending Responses",
                                        value: format!("{}", info.pending_responses),
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "text-slate-500 text-sm",
                                "NTP service not available. Party not started."
                            }
                        }
                    }
                }
            }
        }
    }
}

fn format_optional_micros(value: Option<i64>) -> String {
    value
        .map(|micros| format!("{micros} µs"))
        .unwrap_or_else(|| "n/a".to_string())
}

#[allow(non_snake_case)]
#[component]
fn DebugInfoItem(label: String, value: String) -> Element {
    rsx! {
        div {
            class: "p-3 rounded-lg bg-slate-800/50 border border-slate-700/50",
            div {
                class: "text-xs text-slate-500 mb-1",
                "{label}"
            }
            div {
                class: "text-sm font-mono text-slate-300",
                "{value}"
            }
        }
    }
}
