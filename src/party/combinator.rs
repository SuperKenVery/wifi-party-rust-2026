//! Pipeline combinators for audio routing.
//!
//! Provides utilities for splitting, switching, and mixing audio streams.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
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

pub struct LoopbackSwitch<S> {
    sink: S,
    enabled: Arc<AtomicBool>,
}

impl<S> LoopbackSwitch<S> {
    pub fn new(sink: S, enabled: Arc<AtomicBool>) -> Self {
        Self { sink, enabled }
    }
}

impl<T, S> Sink for LoopbackSwitch<S>
where
    T: Send,
    S: Sink<Input = T>,
{
    type Input = T;

    fn push(&self, input: Self::Input) {
        if self.enabled.load(Ordering::Relaxed) {
            self.sink.push(input);
        }
    }
}

pub struct MixingSource<A, B, Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    a: A,
    b: B,
    _marker: std::marker::PhantomData<Sample>,
}

impl<A, B, Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    MixingSource<A, B, Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(a: A, b: B) -> Self {
        Self {
            a,
            b,
            _marker: std::marker::PhantomData,
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

    fn pull(&self) -> Option<Self::Output> {
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
