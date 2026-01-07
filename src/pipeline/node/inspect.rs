use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::PushNode;

/// A node that calls a closure with each audio frame that passes through it.
/// This is useful for monitoring, logging, or visualization without modifying the audio data.
pub struct InspectNode<F> {
    callback: F,
}

impl<F> InspectNode<F> {
    /// Create a new CallbackNode with the given closure.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next, F, Sample>
    PushNode<CHANNELS, SAMPLE_RATE, Sample, Next> for InspectNode<F>
where
    F: FnMut(&AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>) + Send,
    Next: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
{
    fn push(&mut self, frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, next: &mut Next) {
        // Execute the callback with a reference to the frame
        (self.callback)(&frame);

        // Pass the frame to the next node in the pipeline
        next.push(frame, &mut ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{
        node::{terminal::NullNode, PushNode},
        pipeline::AudioPipeline,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_callback_node() {
        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();

        let frame = AudioBuffer::<f32, 1, 44100>::new(vec![0.0; 100]).unwrap();

        let mut pipeline: AudioPipeline<_, NullNode> =
            AudioPipeline::new(InspectNode::new(move |_frame| {
                *called_clone.lock().unwrap() = true;
            }));

        pipeline.node.push(frame, &mut ());

        assert!(*called.lock().unwrap());
    }
}
