pub mod audio;
pub mod network;

use crate::pipeline::{
    effect::{gain::Gain, noise_gate::NoiseGate},
    node::{
        jitter_buffer::JitterBuffer, mix_pull::MixPullNode, mixer::MixerNode,
        network_push::NetworkPushNode, tee::TeeNode,
    },
    pipeline::AudioPipeline,
};
use crate::state::AppState;
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::info;

use self::audio::{AudioInputNode, AudioOutputNode};
use self::network::NetworkNode;

pub struct Party {
    state: Arc<AppState>,
    network_node: NetworkNode,
    audio_input_node: AudioInputNode,
    audio_output_node: AudioOutputNode,
}

impl Party {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            network_node: NetworkNode::new(),
            audio_input_node: AudioInputNode::new(),
            audio_output_node: AudioOutputNode::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        info!("Starting Party pipelines...");

        todo!()
    }
}
