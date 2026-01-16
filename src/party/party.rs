//! Main party orchestrator.
//!
//! Coordinates audio capture, network transport, and playback into a complete
//! audio sharing pipeline.

use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use crate::audio::AudioSample;
use crate::audio::frame::{AudioBuffer, AudioFrame};
use crate::io::{AudioInput, AudioOutput};
use crate::pipeline::effect::LevelMeter;
use crate::pipeline::node::SimpleBuffer;
use crate::pipeline::{Sink, Source};
use crate::state::AppState;

use super::codec::{FramePacker, FrameUnpacker};
use super::combinator::{MixingSource, Switch, Tee};
use super::host::HostPipelineManager;
use super::network::NetworkNode;

pub struct Party<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
    network_node: NetworkNode<Sample, CHANNELS, SAMPLE_RATE>,
    pipeline_manager: Arc<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>,
    _audio_streams: Vec<cpal::Stream>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            network_node: NetworkNode::new(),
            pipeline_manager: Arc::new(HostPipelineManager::new()),
            _audio_streams: Vec::new(),
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

        let loopback_buffer: SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE> = SimpleBuffer::new();

        // Mic -> LevelMeter -> MicSwitch -> Tee -> FramePacker -> NetworkSink
        //                                      -> LoopbackSwitch -> loopback_buffer
        let mic_sink = Tee::new(
            network_sink.get_data_from(FramePacker::<Sample, CHANNELS, SAMPLE_RATE>::new()),
            loopback_buffer.clone().get_data_from(
                Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(self.state.loopback_enabled.clone()),
            ),
        )
        .get_data_from(Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(
            self.state.mic_enabled.clone(),
        ))
        .get_data_from(LevelMeter::<Sample, CHANNELS, SAMPLE_RATE>::new(
            self.state.mic_audio_level.clone(),
        ));
        let audio_input = AudioInput::new(mic_sink);
        let input_stream = audio_input.start()?;

        // Network (with per-host jitter buffers) -> FrameUnpacker -> MixingSource -> Speaker
        let network_to_speaker =
            network_source.give_data_to(FrameUnpacker::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let speaker_source: MixingSource<_, _, Sample, CHANNELS, SAMPLE_RATE> =
            MixingSource::new(network_to_speaker, loopback_buffer);

        let audio_output: AudioOutput<_> = AudioOutput::new(speaker_source);
        let output_stream = audio_output.start()?;

        self._audio_streams = vec![input_stream, output_stream];

        info!("Party pipelines configured successfully");

        Ok(())
    }
}
