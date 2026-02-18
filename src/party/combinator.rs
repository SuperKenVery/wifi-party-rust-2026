//! Pipeline combinators for audio routing.
//!
//! Provides utilities for splitting, switching, and mixing audio streams:
//! - [`Tee`] - Splits data to two destinations
//! - [`Mixer`] - Static mixer with compile-time known sources
//! - [`DynamicMixer`] - Runtime-configurable mixer using DashMap

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Pullable, Sink, Source};

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
/// This is a static mixer - all sources are provided at construction time.
/// For runtime-configurable mixing, use [`DynamicMixer`].
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

pub type InputId = u64;

/// A runtime-configurable audio mixer using DashMap.
///
/// Supports adding and removing inputs at runtime without locking.
/// Implements both [`Source`] (for backward compat with static pipelines)
/// and [`Pullable`] (for dynamic graph usage).
///
/// # Thread Safety
///
/// Uses `DashMap` for concurrent read/write access. The speaker callback
/// can safely pull while the network thread adds/removes inputs.
pub struct DynamicMixer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    inputs: DashMap<InputId, Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>>,
    next_id: AtomicU64,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    DynamicMixer<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            inputs: DashMap::new(),
            next_id: AtomicU64::new(0),
        }
    }

    pub fn add_input(
        &self,
        source: Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>,
    ) -> InputId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.inputs.insert(id, source);
        id
    }

    pub fn remove_input(&self, id: InputId) -> bool {
        self.inputs.remove(&id).is_some()
    }

    pub fn input_count(&self) -> usize {
        self.inputs.len()
    }

    fn pull_and_mix(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let buffers: Vec<_> = self
            .inputs
            .iter()
            .filter_map(|entry| entry.value().pull(len))
            .collect();

        if buffers.is_empty() {
            return None;
        }

        tracing::trace!(
            "DynamicMixer: pulled {} buffers from {} inputs",
            buffers.len(),
            self.inputs.len()
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

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for DynamicMixer<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for DynamicMixer<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}
