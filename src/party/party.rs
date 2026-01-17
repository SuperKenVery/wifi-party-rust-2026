//! Main party orchestrator.
//!
//! Coordinates audio capture, network transport, and playback into a complete
//! audio sharing pipeline.

use std::sync::Arc;

use anyhow::Result;
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::io::{AudioInput, AudioOutput, LoopbackInput};
use crate::pipeline::effect::LevelMeter;
use crate::pipeline::node::{AudioBatcher, SimpleBuffer};
use crate::pipeline::Sink;
use crate::state::AppState;

use super::combinator::{MixingSource, Switch, Tee};
use super::network::NetworkNode;
use super::stream::{NetworkPacket, RealtimeAudioStream, RealtimeFramePacker, RealtimeStreamId};

pub struct Party<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
    network_node: NetworkNode<Sample, CHANNELS, SAMPLE_RATE>,
    realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    _audio_streams: Vec<cpal::Stream>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            network_node: NetworkNode::new(),
            realtime_stream: Arc::new(RealtimeAudioStream::new()),
            _audio_streams: Vec::new(),
        }
    }
}

impl<Sample: AudioSample + Clone + cpal::SizedSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Party<Sample, CHANNELS, SAMPLE_RATE>
where
    NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
    NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>: rkyv::Archive,
    <NetworkPacket<Sample, CHANNELS, SAMPLE_RATE> as rkyv::Archive>::Archived: rkyv::Deserialize<
            NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        >,
{
    pub fn run(&mut self) -> Result<()> {
        info!("Starting Party pipelines...");

        let (network_sink, realtime_stream) = self
            .network_node
            .start(self.realtime_stream.clone(), self.state.clone())?;

        let loopback_buffer: SimpleBuffer<Sample, CHANNELS, SAMPLE_RATE> = SimpleBuffer::new();

        // ============================================================
        // Mic Pipeline: Mic -> LevelMeter -> MicSwitch -> Tee
        //                                                  -> RealtimeFramePacker(Mic) -> NetworkSink
        //                                                  -> LoopbackSwitch -> loopback_buffer
        // ============================================================
        let mic_packer = RealtimeFramePacker::<Sample, CHANNELS, SAMPLE_RATE>::new(RealtimeStreamId::Mic);
        let mic_sink = Tee::new(
            network_sink.clone().get_data_from(mic_packer),
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

        // ============================================================
        // System Audio Pipeline: SystemAudio -> LevelMeter -> SystemSwitch -> AudioBatcher -> RealtimeFramePacker(System) -> NetworkSink
        // ============================================================
        let system_packer = RealtimeFramePacker::<Sample, CHANNELS, SAMPLE_RATE>::new(RealtimeStreamId::System);
        let system_sink = network_sink
            .get_data_from(system_packer)
            .get_data_from(AudioBatcher::<Sample, CHANNELS, SAMPLE_RATE>::new(5))
            .get_data_from(Switch::<Sample, CHANNELS, SAMPLE_RATE>::new(
                self.state.system_audio_enabled.clone(),
            ))
            .get_data_from(LevelMeter::<Sample, CHANNELS, SAMPLE_RATE>::new(
                self.state.system_audio_level.clone(),
            ));

        let system_stream_result = LoopbackInput::new(system_sink).start();
        let system_stream = match system_stream_result {
            Ok(stream) => Some(stream),
            Err(e) => {
                warn!(
                    "Failed to start system audio capture: {}. \
                     System audio sharing will be disabled.",
                    e
                );
                None
            }
        };

        // ============================================================
        // Output Pipeline: RealtimeAudioStream (mixed from all hosts/streams)
        //                  + loopback_buffer -> Speaker
        // ============================================================
        let speaker_source: MixingSource<_, _, Sample, CHANNELS, SAMPLE_RATE> =
            MixingSource::new(realtime_stream, loopback_buffer);

        let audio_output: AudioOutput<_> = AudioOutput::new(speaker_source);
        let output_stream = audio_output.start()?;

        let mut streams = vec![input_stream, output_stream];
        if let Some(sys_stream) = system_stream {
            streams.push(sys_stream);
        }
        self._audio_streams = streams;

        info!("Party pipelines configured successfully");

        Ok(())
    }
}
