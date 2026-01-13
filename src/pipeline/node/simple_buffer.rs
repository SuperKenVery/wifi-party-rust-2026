//! A simple thread-safe FIFO buffer for generic data.

use super::{Sink, Source};
use crate::pipeline::graph::{PipelineGraph, Inspectable};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SimpleBuffer<T> {
    queue: Arc<Mutex<VecDeque<T>>>,
}

impl<T> SimpleBuffer<T> {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<T> Default for SimpleBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Inspectable for SimpleBuffer<T> {
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", Arc::as_ptr(&self.queue));
        let count = self.queue.lock().unwrap().len();
        
        let svg = format!(
            r#"<div class="w-full h-full bg-orange-900 border border-orange-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-orange-200 mb-1">Buffer</div>
                <div class="text-[10px] font-mono text-orange-300">Count: {}</div>
            </div>"#,
            count
        );
        
        graph.add_node(id.clone(), svg);
        id
    }
}

impl<T: Send + Sync> Sink for SimpleBuffer<T> {
    type Input = T;

    fn push(&self, input: T) {
        self.queue.lock().unwrap().push_back(input);
    }
}

impl<T: Send + Sync> Source for SimpleBuffer<T> {
    type Output = T;

    fn pull(&self) -> Option<T> {
        self.queue.lock().unwrap().pop_front()
    }
}
