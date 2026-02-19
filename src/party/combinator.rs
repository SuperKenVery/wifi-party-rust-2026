//! Pipeline combinators for audio routing.
//!
//! Provides utilities for splitting and mixing audio streams:
//! - [`Tee`] - Splits data to two destinations (implements `Pushable`)
//! - [`DynamicMixer`] - Runtime-configurable mixer using DashMap (implements `Pullable`)

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Pullable, Pushable};

/// Splits pushed data to two destinations.
///
/// When data is pushed to a `Tee`, it clones the data and pushes to both
/// destination A and destination B.
pub struct Tee<T, A, B>
where
    A: Pushable<T>,
    B: Pushable<T>,
{
    a: A,
    b: B,
    _marker: std::marker::PhantomData<T>,
}

impl<T, A, B> Tee<T, A, B>
where
    A: Pushable<T>,
    B: Pushable<T>,
{
    pub fn new(a: A, b: B) -> Self {
        Self {
            a,
            b,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Clone + Send + Sync, A: Pushable<T>, B: Pushable<T>> Pushable<T> for Tee<T, A, B> {
    fn push(&self, input: T) {
        self.a.push(input.clone());
        self.b.push(input);
    }
}

pub type InputId = u64;

/// An audio mixer.
///
/// Supports adding and removing inputs at runtime without locking.
/// Implements [`Pullable`] for pulling mixed audio from all inputs.
///
/// # Usage
///
/// ```ignore
/// // For runtime dynamic cases (e.g., per-host decode chains)
/// let mixer = Arc::new(DynamicMixer::new());
/// let id = mixer.add_input(source);
/// // later: mixer.remove_input(id);
///
/// // For declarative construction with known inputs
/// let mixer = DynamicMixer::with_inputs([source1, source2, source3]);
/// ```
pub struct Mixer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    inputs: DashMap<InputId, Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>>,
    next_id: AtomicU64,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Mixer<Sample, CHANNELS, SAMPLE_RATE>
{
    /// Creates an empty mixer for runtime dynamic input management.
    pub fn new() -> Self {
        Self {
            inputs: DashMap::new(),
            next_id: AtomicU64::new(0),
        }
    }

    /// Creates a mixer with the given inputs (declarative construction).
    ///
    /// Use this when all inputs are known at construction time.
    /// For runtime dynamic cases, use [`new()`](Self::new) + [`add_input()`](Self::add_input).
    pub fn with_inputs(
        inputs: impl IntoIterator<Item = Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>>,
    ) -> Arc<Self> {
        let mixer = Self::new();
        for input in inputs {
            mixer.add_input(input);
        }
        Arc::new(mixer)
    }

    /// Adds an input at runtime. Returns ID for later removal.
    pub fn add_input(
        &self,
        source: Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>,
    ) -> InputId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.inputs.insert(id, source);
        id
    }

    /// Removes an input by ID. Returns true if the input was found and removed.
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
    for Mixer<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> for Mixer<Sample, CHANNELS, SAMPLE_RATE>
{
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        self.pull_and_mix(len)
    }
}
