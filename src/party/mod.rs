pub mod audio;
pub mod network;

use crate::audio::frame::AudioBuffer;
use crate::audio::{AudioFrame, AudioSample};
use crate::pipeline::node::SimpleBuffer;
use crate::pipeline::{Node, Sink, Source};
use crate::state::AppState;
use anyhow::Result;
use std::marker::PhantomData;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::info;

use self::audio::{AudioInput, AudioOutput};
use self::network::NetworkNode;

pub struct Encoder<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sequence_number: u64,
    _marker: PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Encoder<Sample, CHANNELS, SAMPLE_RATE> {
    pub fn new() -> Self {
        Self {
            sequence_number: 0,
            _marker: PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for Encoder<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Encoder<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioFrame;

    fn process(&mut self, input: Self::Input) -> Option<Self::Output> {
        let samples: Vec<i16> = input
            .data()
            .iter()
            .map(|&s| i16::convert_from(s))
            .collect();

        self.sequence_number += 1;
        AudioFrame::new(self.sequence_number, samples).ok()
    }
}

pub struct Decoder<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    _marker: PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Decoder<Sample, CHANNELS, SAMPLE_RATE> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for Decoder<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Decoder<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioFrame;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&mut self, input: Self::Input) -> Option<Self::Output> {
        let samples: Vec<Sample> = input
            .samples
            .data()
            .iter()
            .map(|&s| Sample::convert_from(s))
            .collect();

        AudioBuffer::new(samples).ok()
    }
}

pub struct Tee<A, B> {
    a: A,
    b: B,
}

impl<A, B> Tee<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<T, A, B> Sink for Tee<A, B>
where
    T: Clone + Send,
    A: Sink<Input = T>,
    B: Sink<Input = T>,
{
    type Input = T;

    fn push(&mut self, input: Self::Input) {
        self.a.push(input.clone());
        self.b.push(input);
    }
}

pub struct LoopbackSwitch<S> {
    sink: S,
    enabled: Arc<std::sync::atomic::AtomicBool>,
}

impl<S> LoopbackSwitch<S> {
    pub fn new(sink: S, enabled: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { sink, enabled }
    }
}

impl<T, S> Sink for LoopbackSwitch<S>
where
    T: Send,
    S: Sink<Input = T>,
{
    type Input = T;

    fn push(&mut self, input: Self::Input) {
        if self.enabled.load(Ordering::Relaxed) {
            self.sink.push(input);
        }
    }
}

pub struct MixingSource<A, B, Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    a: A,
    b: B,
    _marker: PhantomData<Sample>,
}

impl<A, B, Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    MixingSource<A, B, Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(a: A, b: B) -> Self {
        Self {
            a,
            b,
            _marker: PhantomData,
        }
    }
}

impl<A, B, Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for MixingSource<A, B, Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
    A: Source<Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>,
    B: Source<Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>,
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&mut self) -> Option<Self::Output> {
        match (self.a.pull(), self.b.pull()) {
            (Some(a), Some(b)) => {
                let mixed: Vec<Sample> = a
                    .data()
                    .iter()
                    .zip(b.data().iter())
                    .map(|(&x, &y)| {
                        let sum = x.to_f64_normalized() + y.to_f64_normalized();
                        Sample::from_f64_normalized(sum)
                    })
                    .collect();
                AudioBuffer::new(mixed).ok()
            }
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }
}

pub struct Party {
    state: Arc<AppState>,
    network_node: NetworkNode,
}

impl Party {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            network_node: NetworkNode::new(),
        }
    }

    pub fn run<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self) -> Result<()>
    where
        Sample: AudioSample + Clone,
    {
        info!("Starting Party pipelines...");

        let (network_sink, network_source) = self.network_node.start(self.state.clone())?;

        let loopback_buffer: SimpleBuffer<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> =
            SimpleBuffer::new();

        let loopback_sink =
            LoopbackSwitch::new(loopback_buffer.clone(), self.state.loopback_enabled.clone());

        let mic_to_network = network_sink.pipe(Encoder::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let mic_sink = Tee::new(mic_to_network, loopback_sink);

        let _audio_input: AudioInput<_> = AudioInput::new(mic_sink);

        let network_to_speaker = network_source.pipe(Decoder::<Sample, CHANNELS, SAMPLE_RATE>::new());
        let speaker_source: MixingSource<_, _, Sample, CHANNELS, SAMPLE_RATE> =
            MixingSource::new(network_to_speaker, loopback_buffer);

        let _audio_output: AudioOutput<_> = AudioOutput::new(speaker_source);

        info!("Party pipelines configured successfully");

        Ok(())
    }
}
