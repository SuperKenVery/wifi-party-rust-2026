//! GraphNode wrapper for dynamic graph construction.
//!
//! [`GraphNode`] wraps any [`Node`] to implement the dynamic traits
//! ([`Pushable`], [`Pullable`]), enabling runtime graph modification.
//!
//! # Usage
//!
//! ```ignore
//! let decoder = GraphNode::new(OpusDecoder::new()?);
//! let jitter_buffer = Arc::new(JitterBuffer::new(64));
//!
//! // Connect decoder output to jitter buffer
//! decoder.add_output(0, jitter_buffer.clone());
//!
//! // Push data through the chain
//! decoder.push(opus_packet); // -> decode -> jitter_buffer.push()
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use dashmap::DashMap;

use super::dyn_traits::{Pullable, Pushable};
use super::traits::Node;

pub type OutputId = u64;

/// Wraps a [`Node`] to implement dynamic pipeline traits.
///
/// `GraphNode` stores:
/// - Output destinations (`DashMap`) - where to forward processed output on push
/// - Input source (`RwLock`) - where to pull input from on pull
///
/// # Push Behavior (default)
/// 1. Process input through the wrapped node
/// 2. Forward output to all connected output destinations
///
/// # Pull Behavior (default)
/// 1. Pull input from connected input source
/// 2. Process through the wrapped node
/// 3. Return output
///
/// # Thread Safety
/// - Uses `DashMap` for outputs (concurrent read/write without explicit locking)
/// - Uses `RwLock` for input source (single input, read-heavy access pattern)
pub struct GraphNode<N: Node> {
    node: N,
    outputs: DashMap<OutputId, Arc<dyn Pushable<N::Output>>>,
    input: RwLock<Option<Arc<dyn Pullable<N::Input>>>>,
    next_output_id: AtomicU64,
}

impl<N: Node> GraphNode<N> {
    pub fn new(node: N) -> Self {
        Self {
            node,
            outputs: DashMap::new(),
            input: RwLock::new(None),
            next_output_id: AtomicU64::new(0),
        }
    }

    pub fn add_output(&self, dest: Arc<dyn Pushable<N::Output>>) -> OutputId {
        let id = self.next_output_id.fetch_add(1, Ordering::Relaxed);
        self.outputs.insert(id, dest);
        id
    }

    pub fn remove_output(&self, id: OutputId) -> Option<Arc<dyn Pushable<N::Output>>> {
        self.outputs.remove(&id).map(|(_, v)| v)
    }

    pub fn set_input(&self, source: Arc<dyn Pullable<N::Input>>) {
        let mut input = self.input.write().unwrap();
        *input = Some(source);
    }

    pub fn clear_input(&self) {
        let mut input = self.input.write().unwrap();
        *input = None;
    }

    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }
}

impl<N: Node> Pushable<N::Input> for GraphNode<N>
where
    N::Output: Clone,
{
    fn push(&self, input: N::Input) {
        if let Some(output) = self.node.process(input) {
            for entry in self.outputs.iter() {
                entry.value().push(output.clone());
            }
        }
    }
}

impl<N: Node> Pullable<N::Output> for GraphNode<N> {
    fn pull(&self, len: usize) -> Option<N::Output> {
        let input_source = self.input.read().unwrap();
        let input = input_source.as_ref()?.pull(len)?;
        self.node.process(input)
    }
}
