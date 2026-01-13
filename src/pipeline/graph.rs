
use std::collections::HashMap;

/// A trait for components that can be inspected and visualized in the pipeline graph.
/// This method is called periodically to generate the current visual state of the graph.
pub trait Inspectable: Send + Sync {
    /// Populates the graph with this component's visual representation and returns its ID.
    ///
    /// # Arguments
    /// * `graph` - The graph builder to add nodes and edges to.
    ///
    /// # Returns
    /// The unique ID of the node added (or the "output" node ID if this is a chain).
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        // Default visualization: A simple box with the type name (if we could get it, but we can't easily)
        // So we just use "Node" and the pointer.
        let svg = format!(
            r#"<div class="w-full h-full bg-gray-800 border border-gray-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-gray-400 mb-1">Node</div>
                <div class="text-[10px] font-mono text-gray-500">{}</div>
            </div>"#,
            &id[..8]
        );
        graph.add_node(id.clone(), svg);
        id
    }
}

#[derive(Clone, Default, Debug)]
pub struct PipelineGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub content: String, // SVG/HTML content
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

impl PipelineGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
    }

    pub fn add_node(&mut self, id: String, content: String) {
        if !self.nodes.iter().any(|n| n.id == id) {
            self.nodes.push(GraphNode { id, content });
        }
    }

    pub fn add_edge(&mut self, from: String, to: String, label: Option<String>) {
        if !self.edges.iter().any(|e| e.from == from && e.to == to) {
            self.edges.push(GraphEdge { from, to, label });
        }
    }
}
