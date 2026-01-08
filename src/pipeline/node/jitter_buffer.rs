//! A thread-safe buffer that bridges push and pull pipelines.

use super::{Sink, Source};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A thread-safe FIFO buffer that implements both [`Source`] and [`Sink`].
///
/// `JitterBuffer` serves as a bridge between push-based and pull-based pipelines.
/// It can be cloned (via internal `Arc`) to share between threads - one side
/// pushing data in, another side pulling data out.
///
/// # Example
///
/// ```ignore
/// let buffer = JitterBuffer::<AudioFrame>::new();
///
/// // Clone for use in different pipelines
/// let push_side = buffer.clone();
/// let pull_side = buffer.clone();
///
/// // Push pipeline feeds into buffer
/// push_side.push(frame);
///
/// // Pull pipeline reads from buffer
/// if let Some(frame) = pull_side.pull() {
///     // process frame
/// }
/// ```
#[derive(Clone)]
pub struct JitterBuffer<T> {
    queue: Arc<Mutex<VecDeque<T>>>,
}

impl<T> JitterBuffer<T> {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<T> Default for JitterBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send> Sink for JitterBuffer<T> {
    type Input = T;

    fn push(&mut self, input: T) {
        self.queue.lock().unwrap().push_back(input);
    }
}

impl<T: Send> Source for JitterBuffer<T> {
    type Output = T;

    fn pull(&mut self) -> Option<T> {
        self.queue.lock().unwrap().pop_front()
    }
}
