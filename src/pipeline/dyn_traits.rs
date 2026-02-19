//! Object-safe dynamic pipeline traits.
//!
//! These traits enable heterogeneous collections of pipeline components and
//! runtime graph modification. All pipeline construction uses these dynamic traits.
//!
//! # Traits
//!
//! - [`Pushable<T>`] - Can receive pushed data
//! - [`Pullable<T>`] - Can return data when pulled
//!
//! # Pipeline Construction
//!
//! Use [`push_chain!`] and [`pull_chain!`] macros to build pipelines:
//!
//! ```ignore
//! // Push chain: data flows left-to-right through nodes to sink
//! let pipeline = push_chain![
//!     LevelMeter::new(level.clone()),
//!     Gain::new(volume.clone()),
//!     => network_sink.clone()
//! ];
//!
//! // Pull chain: data is pulled right-to-left from source through nodes
//! let pipeline = pull_chain![
//!     source.clone() =>,
//!     Switch::new(enabled.clone())
//! ];
//! ```

use std::sync::Arc;

/// Passive receiver - can receive pushed data.
///
/// This is the input interface for nodes in a push-based data flow.
/// When data is pushed, the implementation decides what to do with it:
/// - Process and forward to outputs (default for [`GraphNode`])
/// - Store in a buffer (e.g., [`JitterBuffer`])
pub trait Pushable<T>: Send + Sync {
    fn push(&self, input: T);
}

/// Passive producer - can return data when pulled.
///
/// This is the output interface for nodes in a pull-based data flow.
/// When pulled, the implementation decides how to produce data:
/// - Pull from upstream, process, return (default for [`GraphNode`])
/// - Read from internal buffer (e.g., [`JitterBuffer`])
pub trait Pullable<T>: Send + Sync {
    fn pull(&self, len: usize) -> Option<T>;
}

impl<T: Send + Sync> Pushable<T> for Arc<dyn Pushable<T>> {
    fn push(&self, input: T) {
        (**self).push(input)
    }
}

impl<T: Send + Sync> Pullable<T> for Arc<dyn Pullable<T>> {
    fn pull(&self, len: usize) -> Option<T> {
        (**self).pull(len)
    }
}

/// Creates a push chain from nodes, connecting them via GraphNode wrappers.
/// Returns an `Arc<dyn Pushable<FirstNode::Input>>` pointing to the first node.
///
/// # Syntax
///
/// ```ignore
/// push_chain![node1, node2, ..., => sink]
/// ```
///
/// - Nodes are automatically wrapped in `GraphNode`
/// - The `=>` marks the final destination (must implement `Pushable`)
/// - Data flows: input -> node1 -> node2 -> ... -> sink
///
/// # Example
///
/// ```ignore
/// let pipeline = push_chain![
///     LevelMeter::new(level.clone()),
///     Gain::new(volume.clone()),
///     => network_sink.clone()
/// ];
/// pipeline.push(audio_buffer); // flows through LevelMeter -> Gain -> network_sink
/// ```
#[macro_export]
macro_rules! push_chain {
    (=> $sink:expr) => {{
        let sink: std::sync::Arc<dyn $crate::pipeline::Pushable<_>> = $sink;
        sink
    }};

    ($node:expr, $($rest:tt)+) => {{
        let node = std::sync::Arc::new($crate::pipeline::GraphNode::new($node));
        let rest = $crate::push_chain!($($rest)+);
        node.add_output(rest);
        node as std::sync::Arc<dyn $crate::pipeline::Pushable<_>>
    }};
}

/// Creates a pull chain from a source through nodes, connecting them via GraphNode wrappers.
/// Returns an `Arc<dyn Pullable<LastNode::Output>>` pointing to the last node.
///
/// # Syntax
///
/// ```ignore
/// pull_chain![source =>, node1, node2, ...]
/// ```
///
/// - The `=>` marks the source (must implement `Pullable`)
/// - Nodes are automatically wrapped in `GraphNode`
/// - Data flows: source -> node1 -> node2 -> ... -> output
///
/// # Example
///
/// ```ignore
/// let pipeline = pull_chain![
///     audio_source.clone() =>,
///     Switch::new(enabled.clone())
/// ];
/// let data = pipeline.pull(256); // pulls from audio_source -> Switch -> output
/// ```
#[macro_export]
macro_rules! pull_chain {
    ($source:expr =>) => {{
        let source: std::sync::Arc<dyn $crate::pipeline::Pullable<_>> = $source;
        source
    }};

    ($source:expr =>, $node:expr $(, $($rest:tt)*)?) => {{
        let source: std::sync::Arc<dyn $crate::pipeline::Pullable<_>> = $source;
        let node = std::sync::Arc::new($crate::pipeline::GraphNode::new($node));
        node.set_input(source);
        $crate::pull_chain!(node as std::sync::Arc<dyn $crate::pipeline::Pullable<_>> => $(, $($rest)*)?)
    }};

    ($wrapped:expr => $(, $node:expr $(, $($rest:tt)*)?)?) => {{
        $(
            let next = std::sync::Arc::new($crate::pipeline::GraphNode::new($node));
            next.set_input($wrapped);
            $crate::pull_chain!(next as std::sync::Arc<dyn $crate::pipeline::Pullable<_>> => $(, $($rest)*)?)
        )?
        $wrapped
    }};
}
