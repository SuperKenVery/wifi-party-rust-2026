use crate::pipeline::graph::PipelineGraph;
use crate::state::{AppState, ConnectionStatus, HostInfo};
use dioxus::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

#[allow(non_snake_case)]
pub fn App() -> Element {
    let state_arc = use_context::<Arc<AppState>>();

    // Create signals for reactive UI
    let mut connection_status = use_signal(|| ConnectionStatus::Disconnected);
    let mut active_hosts = use_signal(|| Vec::<HostInfo>::new());
    let mut mic_muted = use_signal(|| false);
    let mut mic_volume = use_signal(|| 1.0f32);
    let mut mic_audio_level = use_signal(|| 0.0f32);
    let mut loopback_enabled = use_signal(|| false);
    let mut local_host_id = use_signal(|| String::from("Unknown"));

    // Poll state periodically
    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            loop {
                // Update connection status
                if let Ok(status) = state.connection_status.lock() {
                    connection_status.set(*status);
                }

                // Update active hosts
                if let Ok(infos) = state.host_infos.lock() {
                    active_hosts.set(infos.clone());
                }

                // Update mic muted status
                mic_muted.set(state.mic_muted.load(std::sync::atomic::Ordering::Relaxed));

                // Update mic volume
                if let Ok(vol) = state.mic_volume.lock() {
                    mic_volume.set(*vol);
                }

                // Update mic audio level
                if let Ok(level) = state.mic_audio_level.lock() {
                    let new_level = *level;
                    mic_audio_level.set(new_level);
                }

                // Update loopback status
                loopback_enabled.set(
                    state
                        .loopback_enabled
                        .load(std::sync::atomic::Ordering::Relaxed),
                );

                // Update local host ID
                if let Ok(id_opt) = state.local_host_id.lock() {
                    if let Some(id) = *id_opt {
                        local_host_id.set(id.to_string());
                    }
                }

                // Force graph redraw if needed (though graph structure is static, stats might update)
                // In a real implementation we would have a separate signal for graph stats
                // For now, let's just trigger a re-render if the graph lock is available
                // (Note: This is a bit hacky, normally we'd clone specific stats)

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    });

    rsx! {
        div {
            class: "container mx-auto p-4",

            // Header
            Header {
                connection_status: connection_status(),
                local_host_id: local_host_id(),
                participant_count: active_hosts().len(),
            }

            // Self Audio Section
            SelfAudioSection {
                mic_muted: mic_muted(),
                mic_volume: mic_volume(),
                mic_audio_level: mic_audio_level(),
                loopback_enabled: loopback_enabled(),
            }

            // Participants Section
            ParticipantsSection {
                hosts: active_hosts(),
            }

            // Pipeline Visualization
            PipelineVisualization {}

            // Statistics Panel
            StatisticsPanel {}
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn Header(
    connection_status: ConnectionStatus,
    local_host_id: String,
    participant_count: usize,
) -> Element {
    let status_text = match connection_status {
        ConnectionStatus::Connected => "Connected",
        ConnectionStatus::Disconnected => "Disconnected",
    };

    let status_color = match connection_status {
        ConnectionStatus::Connected => "text-green-500",
        ConnectionStatus::Disconnected => "text-red-500",
    };

    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg mb-6",

            h1 {
                class: "text-3xl font-bold mb-4",
                "🎤 Wi-Fi Party KTV"
            }

            div {
                class: "flex items-center gap-4",

                div {
                    span { class: "text-gray-400", "Status: " }
                    span { class: status_color, "{status_text}" }
                }

                div {
                    span { class: "text-gray-400", "Host ID: " }
                    span { class: "font-mono", "{local_host_id}" }
                }

                div {
                    span { class: "text-gray-400", "Participants: " }
                    span { class: "font-bold", "{participant_count}" }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn SelfAudioSection(
    mic_muted: bool,
    mic_volume: f32,
    mic_audio_level: f32,
    loopback_enabled: bool,
) -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    let state_clone = state_arc.clone();
    let on_mute_toggle = move |_| {
        let current = state_clone
            .mic_muted
            .load(std::sync::atomic::Ordering::Relaxed);
        state_clone
            .mic_muted
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let state_clone2 = state_arc.clone();
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            let volume = value_str / 100.0;
            if let Ok(mut vol) = state_clone2.mic_volume.lock() {
                *vol = volume;
            }
        }
    };

    let state_clone3 = state_arc.clone();
    let on_loopback_toggle = move |_| {
        let current = state_clone3
            .loopback_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        state_clone3
            .loopback_enabled
            .store(!current, std::sync::atomic::Ordering::Relaxed);
    };

    let mute_button_class = if mic_muted {
        "px-6 py-3 bg-red-600 hover:bg-red-700 text-white rounded-lg font-bold"
    } else {
        "px-6 py-3 bg-green-600 hover:bg-green-700 text-white rounded-lg font-bold"
    };

    let mute_button_text = if mic_muted { "Unmute" } else { "Mute" };

    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg mb-6",

            h2 {
                class: "text-2xl font-bold mb-4",
                "Your Audio"
            }

            div {
                class: "flex items-center gap-6",

                button {
                    class: mute_button_class,
                    onclick: on_mute_toggle,
                    "{mute_button_text} Microphone"
                }

                button {
                    class: if loopback_enabled {
                        "px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-bold"
                    } else {
                        "px-6 py-3 bg-gray-600 hover:bg-gray-700 text-white rounded-lg font-bold"
                    },
                    onclick: on_loopback_toggle,
                    if loopback_enabled { "🎧 Loopback: ON" } else { "🎧 Loopback: OFF" }
                }

                div {
                    class: "flex-1",
                    label {
                        class: "block text-sm mb-2",
                        "Microphone Volume: {(mic_volume * 100.0) as i32}%"
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
            }

            div {
                class: "mt-4",
                label {
                    class: "block text-sm mb-2",
                    "🎤 Microphone Level: {(mic_audio_level * 100.0) as i32}%"
                }
                div {
                    class: "relative w-full h-6 bg-gray-700 rounded-lg overflow-hidden border border-gray-600",
                    div {
                        class: "absolute h-full bg-gradient-to-r from-green-500 via-yellow-500 to-red-500 transition-all duration-100",
                        style: "width: {(mic_audio_level * 100.0).min(100.0)}%",
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn ParticipantsSection(hosts: Vec<HostInfo>) -> Element {
    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg mb-6",

            h2 {
                class: "text-2xl font-bold mb-4",
                "Participants ({hosts.len()})"
            }

            if hosts.is_empty() {
                div {
                    class: "text-gray-400 text-center py-8",
                    "No other participants connected"
                }
            } else {
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                    for host in hosts {
                        HostCard {
                            host: host.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn HostCard(host: HostInfo) -> Element {
    let on_volume_change = move |evt: Event<FormData>| {
        if let Ok(value_str) = evt.value().parse::<f32>() {
            let _volume = value_str / 100.0;
        }
    };

    rsx! {
        div {
            class: "bg-gray-700 p-4 rounded-lg",

            div {
                class: "flex items-center justify-between mb-2",

                div {
                    class: "font-mono text-sm",
                    "{host.id.to_string()}"
                }

                div {
                    class: "text-xs text-gray-400",
                    "Loss: {(host.packet_loss * 100.0) as i32}%"
                }
            }

            div {
                label {
                    class: "block text-sm mb-1",
                    "Volume: {(host.volume * 100.0) as i32}%"
                }
                input {
                    r#type: "range",
                    min: 0,
                    max: 200,
                    value: (host.volume * 100.0) as i32,
                    class: "w-full",
                    oninput: on_volume_change,
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn StatisticsPanel() -> Element {
    rsx! {
        div {
            class: "bg-gray-800 text-white p-6 rounded-lg",

            h2 {
                class: "text-2xl font-bold mb-4",
                "Statistics"
            }

            div {
                class: "grid grid-cols-3 gap-4",

                div {
                    class: "text-center",
                    div { class: "text-sm text-gray-400", "Latency" }
                    div { class: "text-2xl font-bold", "~20ms" }
                }

                div {
                    class: "text-center",
                    div { class: "text-sm text-gray-400", "Packet Loss" }
                    div { class: "text-2xl font-bold", "0%" }
                }

                div {
                    class: "text-center",
                    div { class: "text-sm text-gray-400", "Jitter" }
                    div { class: "text-2xl font-bold", "2ms" }
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[component]
fn PipelineVisualization() -> Element {
    let state_arc = use_context::<Arc<AppState>>();
    
    // We need to subscribe to graph updates. 
    // Since graph is in Arc<Mutex<...>>, we can just poll it.
    let mut graph_signal = use_signal(|| PipelineGraph::default());
    
    use_effect(move || {
        let state = state_arc.clone();
        spawn(async move {
            loop {
                // Update graph visualization
                if let Ok(mut g) = state.pipeline_graph.lock() {
                    g.clear();
                    if let Ok(pipelines) = state.pipelines.lock() {
                        for pipeline in pipelines.iter() {
                            pipeline.get_visual(&mut g);
                        }
                    }
                    graph_signal.set(g.clone());
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        });
    });

    let graph = graph_signal();
    
    // Calculate positions using a layered layout (Topological Sort-ish)
    let mut positions = HashMap::new();
    let node_width = 120;
    let node_height = 60;
    let x_spacing = 200;
    let y_spacing = 100;

    // 1. Build Adjacency and In-Degree
    let mut adj: HashMap<&String, Vec<&String>> = HashMap::new();
    let mut in_degree: HashMap<&String, usize> = HashMap::new();
    
    for node in &graph.nodes {
        in_degree.insert(&node.id, 0);
        adj.insert(&node.id, Vec::new());
    }
    
    for edge in &graph.edges {
        adj.entry(&edge.from).or_default().push(&edge.to);
        *in_degree.entry(&edge.to).or_insert(0) += 1;
    }

    // 2. Assign Layers
    let mut layers: HashMap<&String, usize> = HashMap::new();
    let mut queue = std::collections::VecDeque::new();
    
    // Initialize queue with sources (in-degree 0)
    for (id, &deg) in &in_degree {
        if deg == 0 {
            queue.push_back((*id, 0));
            layers.insert(*id, 0);
        }
    }
    
    // Fallback for cycles: if queue is empty but nodes exist, pick the first one
    if queue.is_empty() && !graph.nodes.is_empty() {
        if let Some(node) = graph.nodes.first() {
             queue.push_back((&node.id, 0));
             layers.insert(&node.id, 0);
        }
    }

    // Process queue
    let mut steps = 0;
    while let Some((u, d)) = queue.pop_front() {
        steps += 1;
        if steps > 1000 { break; } // Safety break

        if let Some(neighbors) = adj.get(u) {
            for &v in neighbors {
                let current_layer = *layers.get(v).unwrap_or(&0);
                // If we found a longer path to v, push it further right
                if d + 1 > current_layer {
                    layers.insert(v, d + 1);
                    queue.push_back((v, d + 1));
                }
            }
        }
    }
    
    // 3. Group by layer and sort for stability
    let mut nodes_by_layer: HashMap<usize, Vec<&String>> = HashMap::new();
    for node in &graph.nodes {
        let l = *layers.get(&node.id).unwrap_or(&0);
        nodes_by_layer.entry(l).or_default().push(&node.id);
    }
    
    // 4. Assign Coordinates
    for (layer, ids) in nodes_by_layer.iter_mut() {
        ids.sort(); // Sort by ID for deterministic layout
        for (idx, id) in ids.iter().enumerate() {
            let x = layer * x_spacing + 50;
            let y = idx * y_spacing + 50;
            positions.insert((*id).clone(), (x, y));
        }
    }

    // Generate edges
    let edges = graph.edges.iter().map(|edge| {
        if let (Some(&(x1, y1)), Some(&(x2, y2))) = (positions.get(&edge.from), positions.get(&edge.to)) {
            // Calculate center points
            let start_x = x1 + node_width / 2;
            let start_y = y1 + node_height / 2;
            let end_x = x2 + node_width / 2;
            let end_y = y2 + node_height / 2;
            
            rsx! {
                line {
                    x1: "{start_x}",
                    y1: "{start_y}",
                    x2: "{end_x}",
                    y2: "{end_y}",
                    stroke: "#6B7280", // gray-500
                    stroke_width: "2",
                    marker_end: "url(#arrowhead)"
                }
            }
        } else {
            rsx! {}
        }
    });
    
    // Generate nodes
    let nodes = graph.nodes.iter().map(|node| {
        let (x, y) = positions.get(&node.id).unwrap_or(&(0, 0));
        
        rsx! {
            g {
                key: "{node.id}",
                foreignObject {
                    x: "{x}",
                    y: "{y}",
                    width: "{node_width}",
                    height: "{node_height}",
                    dangerous_inner_html: "{node.content}"
                }
            }
        }
    });

    rsx! {
        div {
            class: "flex-1 bg-gray-900 p-4 rounded-lg shadow-inner overflow-hidden relative",
            svg {
                width: "100%",
                height: "100%",
                view_box: "0 0 800 600",
                defs {
                    marker {
                        id: "arrowhead",
                        marker_width: "10",
                        marker_height: "7",
                        ref_x: "10",
                        ref_y: "3.5",
                        orient: "auto",
                        polygon {
                            points: "0 0, 10 3.5, 0 7",
                            fill: "#6B7280"
                        }
                    }
                }
                {edges}
                {nodes}
            }
        }
    }
}
