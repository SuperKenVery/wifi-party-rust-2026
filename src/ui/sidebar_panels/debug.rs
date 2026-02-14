use crate::party::NtpDebugInfo;
use dioxus::prelude::*;

use super::PanelHeader;

#[allow(non_snake_case)]
#[component]
pub fn DebugPanel(
    ntp_info: Option<NtpDebugInfo>,
    #[props(default)] on_back: Option<EventHandler<()>>,
) -> Element {
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
