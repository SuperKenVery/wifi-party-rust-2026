//! Pipeline combinators for audio routing.
//!
//! Provides utilities for splitting, switching, and mixing audio streams.

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Sink, Source};

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

    fn push(&self, input: Self::Input) {
        self.a.push(input.clone());
        self.b.push(input);
    }
}

pub type BoxedSource<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> =
    Box<dyn Source<Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>;

/// Mixes multiple audio sources together by summing their samples.
///
/// Accepts any iterable of sources via `from_iter`. Sources are stored as boxed
/// trait objects to allow mixing different concrete types.
pub struct Mixer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sources: Vec<BoxedSource<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Mixer<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(sources: Vec<BoxedSource<Sample, CHANNELS, SAMPLE_RATE>>) -> Self {
        Self { sources }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for Mixer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        let buffers: Vec<_> = self.sources.iter().filter_map(|s| s.pull(len)).collect();

        if buffers.is_empty() {
            return None;
        }
        tracing::trace!(
            "Mixer: pulled {} buffers from {} sources",
            buffers.len(),
            self.sources.len()
        );

        if buffers.len() == 1 {
            return Some(buffers.into_iter().next().unwrap());
        }

        let mut mixed: Vec<f64> = vec![0.0; len];

        for buffer in &buffers {
            for (i, sample) in buffer.data().iter().enumerate() {
                if i < len {
                    mixed[i] += sample.to_f64_normalized();
                }
            }
        }

        let result: Vec<Sample> = mixed.into_iter().map(Sample::from_f64_normalized).collect();

        AudioBuffer::new(result).ok()
    }
}
