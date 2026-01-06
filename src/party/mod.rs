use crate::audio::frame::AudioBuffer;
use crate::pipeline::{
    effect::{gain::Gain, noise_gate::NoiseGate},
    node::{
        jitter_buffer::JitterBuffer, mix_pull::MixPullNode, mixer::MixerNode,
        network_push::NetworkPushNode, tee::TeeNode, PullNode, PushNode,
    },
    pipeline::AudioPipeline,
};
use crate::state::AppState;
use rtrb::Producer;
use std::sync::Arc;

/// The main Party struct that manages all audio pipelines.
/// It is generic over the concrete input and output pipeline types
/// to enable full static dispatch.
pub struct Party<Input, Output> {
    pub input_pipeline: Input,
    pub output_pipeline: Output,
}

/// Helper function to construct a fully-typed Party.
pub fn build_party<const CHANNELS: usize, const SAMPLE_RATE: u32>(
    state: Arc<AppState>,
    network_sender: Producer<Vec<u8>>,
) -> Party<impl PushNode<CHANNELS, SAMPLE_RATE>, impl PullNode<CHANNELS, SAMPLE_RATE>> {
    // --- INPUT PIPELINE ---
    // Mic -> NoiseGate -> Gain -> Tee -> (Network, Loopback)
    let loopback_buffer = JitterBuffer::<CHANNELS, SAMPLE_RATE>::new();
    let network_node = NetworkPushNode::new(network_sender, state.clone());
    let tee_node = TeeNode::new(network_node, loopback_buffer.clone());

    let input_pipeline = AudioPipeline::new(tee_node, Gain(1.0))
        .connect(NoiseGate::new(0.01, 1024));

    // --- OUTPUT PIPELINE ---
    // (Mixer, Loopback) -> MixPullNode -> Gain -> Speaker
    let mixer_node = MixerNode::new(state.clone());
    let mix_node = MixPullNode::new(mixer_node, loopback_buffer);
    let output_pipeline = AudioPipeline::new(mix_node, Gain(0.8));

    Party {
        input_pipeline,
        output_pipeline,
    }
}

impl<I, O> Party<I, O> {
    pub fn push_frame<const C: usize, const S: u32>(
        &mut self,
        frame: AudioBuffer<f32, C, S>,
    ) where
        I: PushNode<C, S>,
    {
        self.input_pipeline.push(frame, &mut ());
    }

    pub fn pull_frame<const C: usize, const S: u32>(&mut self) -> Option<AudioBuffer<f32, C, S>>
    where
        O: PullNode<C, S>,
    {
        self.output_pipeline.pull(&mut ())
    }
}
