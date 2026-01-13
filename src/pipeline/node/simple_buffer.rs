//! A simple thread-safe FIFO buffer for generic data.

use super::{Sink, Source};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::debug;

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

impl<T: Send> Sink for SimpleBuffer<T> {
    type Input = T;

    fn push(&self, input: T) {
        self.queue.lock().unwrap().push_back(input);
    }
}

impl<T: Send> Source for SimpleBuffer<T> {
    type Output = T;

    fn pull(&self) -> Option<T> {
        self.queue.lock().unwrap().pop_front()
    }
}
