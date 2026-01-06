use crate::audio::frame::{AudioBuffer, AudioFrame};
use crate::pipeline::node::PushNode;
use crate::state::AppState;
use rtrb::Producer;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::warn;

pub struct NetworkPushNode {
    producer: Producer<Vec<u8>>,
    state: Arc<AppState>,
}

impl NetworkPushNode {
    pub fn new(producer: Producer<Vec<u8>>, state: Arc<AppState>) -> Self {
        Self { producer, state }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next> PushNode<CHANNELS, SAMPLE_RATE, Next>
    for NetworkPushNode
{
    fn push(&mut self, frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, _next: &mut Next) {
        let i16_samples: Vec<i16> = frame
            .data()
            .iter()
            .map(|&s| (s * 32768.0).clamp(-32768.0, 32767.0) as i16)
            .collect();

        let seq = self.state.sequence_number.fetch_add(1, Ordering::Relaxed);

        if let Ok(audio_frame) = AudioFrame::new(seq, i16_samples) {
            if let Ok(serialized) = audio_frame.serialize() {
                if self.producer.push(serialized).is_err() {
                    warn!("Network send queue full, dropping frame");
                }
            }
        }
    }
}
