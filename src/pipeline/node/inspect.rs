use crate::pipeline::node::PushNode;

/// A node that calls a closure with each data item that passes through it.
/// This is useful for monitoring, logging, or visualization without modifying the data.
pub struct InspectNode<F> {
    callback: F,
}

impl<F> InspectNode<F> {
    /// Create a new InspectNode with the given closure.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<T, Next, F> PushNode<Next> for InspectNode<F>
where
    F: FnMut(&T) + Send,
    Next: PushNode<(), Input = T, Output = T>,
{
    type Input = T;
    type Output = T;

    fn push(&mut self, data: T, next: &mut Next) {
        // Execute the callback with a reference to the data
        (self.callback)(&data);

        // Pass the data to the next node in the pipeline
        let mut null = ();
        next.push(data, &mut null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{
        node::{null_node::NullNode, PushNode},
        pipeline::AudioPipeline,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_callback_node() {
        use crate::audio::frame::AudioBuffer;

        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();

        let frame = AudioBuffer::<f32, 1, 44100>::new(vec![0.0; 100]).unwrap();

        type Frame = AudioBuffer<f32, 1, 44100>;
        let mut pipeline: AudioPipeline<_, NullNode<Frame, Frame>> =
            AudioPipeline::new(InspectNode::new(move |_frame| {
                *called_clone.lock().unwrap() = true;
            }));

        let mut null = NullNode::new();
        pipeline.push(frame, &mut null);

        assert!(*called.lock().unwrap());
    }
}
