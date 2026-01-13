//! Main party orchestrator.
//!
//! Coordinates audio capture, network transport, and playback into a complete
//! audio sharing pipeline.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing::info;

use crate::audio::AudioSample;
use crate::audio::frame::{AudioBuffer, AudioFrame};
use crate::io::{AudioInput, AudioOutput};
use crate::pipeline::node::SimpleBuffer;
use crate::pipeline::{Sink, Source};
use crate::state::AppState;

use super::codec::{FramePacker, FrameUnpacker};
use super::combinator::{LoopbackSwitch, MixingSource, Tee};
use super::host::HostPipelineManager;
use super::network::NetworkNode;

pub struct Party<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
    network_node: NetworkNode<Sample, CHANNELS, SAMPLE_RATE>,
    pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            network_node: NetworkNode::new(),
            pipeline_manager: Arc::new(Mutex::new(HostPipelineManager::new())),
        }
    }
}

impl<Sample: AudioSample + Clone + cpal::SizedSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: rkyv::Archive,
    <AudioFrame<Sample, CHANNELS, SAMPLE_RATE> as rkyv::Archive>::Archived: rkyv::Deserialize<
            AudioFrame<Sample, CHANNELS, SAMPLE_RATE>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        >,
{
    pub fn run(&mut self) -> Result<()> {
        info!("Starting Party pipelines...");

        let (network_sink, network_source) = self
            .network_node
            .start(self.pipeline_manager.clone(), self.state.clone())?;

        let loopback_buffer: SimpleBuffer<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> =
            SimpleBuffer::new();

        // Mic -> FramePacker -> NetworkSink
        //     -> LoopbackSwitch -> loopback_buffer
        let mut audio_input = AudioInput::new(Tee::new(
            network_sink.get_data_from(FramePacker::<Sample, CHANNELS, SAMPLE_RATE>::new()),
            loopback_buffer.clone().get_data_from(
                LoopbackSwitch::<Sample, CHANNELS, SAMPLE_RATE>::new(
                    self.state.loopback_enabled.clone(),
                ),
            ),
        ));
        audio_input.start()?;

        // Network (with per-host jitter buffers) -> FrameUnpacker -> MixingSource -> Speaker
        let network_to_speaker =
            network_source.give_data_to(FrameUnpacker::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let speaker_source: MixingSource<_, _, Sample, CHANNELS, SAMPLE_RATE> =
            MixingSource::new(network_to_speaker, loopback_buffer);

        let mut audio_output: AudioOutput<_> = AudioOutput::new(speaker_source);
        audio_output.start()?;

        info!("Party pipelines configured successfully");

        Ok(())
    }
}
