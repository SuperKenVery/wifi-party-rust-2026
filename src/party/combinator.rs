//! Pipeline combinators for audio routing.
//!
//! Provides utilities for splitting and mixing audio streams:
//! - [`Tee`] - Splits data to two destinations (implements `Pushable`)
//! - [`DynamicMixer`] - Runtime-configurable mixer using DashMap (implements `Pullable`)

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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

struct SelectState {
    logical_frames: u64,
    consumed_frames: Vec<u64>,
}

/// Selects one pull input for output while keeping all inputs aligned.
///
/// `pull()` returns audio from the selected input only, but it also advances
/// non-selected inputs by discarding the same number of frames whenever data is
/// available. If an inactive input temporarily has no data, the selector keeps
/// its consumed position behind the logical output position and catches it up
/// before it can be selected later.
pub struct SynchronizedSelect<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    inputs: Vec<Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>>,
    selected: AtomicUsize,
    state: Mutex<SelectState>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    SynchronizedSelect<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        inputs: impl IntoIterator<Item = Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>>,
    ) -> Self {
        let inputs: Vec<_> = inputs.into_iter().collect();
        let input_count = inputs.len();
        Self {
            inputs,
            selected: AtomicUsize::new(0),
            state: Mutex::new(SelectState {
                logical_frames: 0,
                consumed_frames: vec![0; input_count],
            }),
        }
    }

    pub fn set_selected(&self, index: usize) {
        if index < self.inputs.len() {
            self.selected.store(index, Ordering::Release);
        }
    }

    pub fn selected(&self) -> usize {
        self.selected.load(Ordering::Acquire)
    }

    pub fn reset_to(&self, frame_pos: u64) {
        let mut state = self.state.lock().unwrap();
        state.logical_frames = frame_pos;
        for consumed in &mut state.consumed_frames {
            *consumed = frame_pos;
        }
    }

    pub fn discard_to(&self, frame_pos: u64) {
        let mut state = self.state.lock().unwrap();
        state.logical_frames = state.logical_frames.max(frame_pos);
        self.catch_up_all(&mut state);
    }

    fn catch_up_all(&self, state: &mut SelectState) {
        for index in 0..self.inputs.len() {
            let _ = self.catch_up_input(index, state.logical_frames, state);
        }
    }

    fn catch_up_input(&self, index: usize, target_frames: u64, state: &mut SelectState) -> bool {
        while state.consumed_frames[index] < target_frames {
            let frames_to_discard = target_frames - state.consumed_frames[index];
            let samples_to_discard = frames_to_discard.saturating_mul(CHANNELS as u64);
            let Ok(samples_to_discard) = usize::try_from(samples_to_discard) else {
                return false;
            };
            let Some(discarded) = self.inputs[index].pull(samples_to_discard) else {
                return false;
            };
            let discarded_frames = discarded.data().len() / CHANNELS;
            if discarded_frames == 0 {
                return false;
            }
            state.consumed_frames[index] += discarded_frames as u64;
        }

        true
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>
    for SynchronizedSelect<Sample, CHANNELS, SAMPLE_RATE>
{
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let selected = self.selected();
        if selected >= self.inputs.len() || len == 0 {
            return None;
        }

        let mut state = self.state.lock().unwrap();
        let logical_frames = state.logical_frames;
        if !self.catch_up_input(selected, logical_frames, &mut state) {
            return None;
        }

        let output = self.inputs[selected].pull(len)?;
        let output_frames = output.data().len() / CHANNELS;
        if output_frames == 0 {
            return None;
        }

        state.consumed_frames[selected] += output_frames as u64;
        state.logical_frames += output_frames as u64;
        self.catch_up_all(&mut state);

        Some(output)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::SimpleBuffer;

    type TestBuffer = AudioBuffer<f32, 2, 48_000>;

    fn audio(frames: &[(f32, f32)]) -> TestBuffer {
        let mut samples = Vec::with_capacity(frames.len() * 2);
        for (left, right) in frames {
            samples.push(*left);
            samples.push(*right);
        }
        AudioBuffer::new(samples).unwrap()
    }

    fn make_selector(
        raw: &SimpleBuffer<f32, 2, 48_000>,
        alternate: &SimpleBuffer<f32, 2, 48_000>,
    ) -> SynchronizedSelect<f32, 2, 48_000> {
        SynchronizedSelect::new(vec![
            Arc::new(raw.clone()) as Arc<dyn Pullable<TestBuffer>>,
            Arc::new(alternate.clone()) as Arc<dyn Pullable<TestBuffer>>,
        ])
    }

    #[test]
    fn synchronized_select_consumes_inactive_input() {
        let raw = SimpleBuffer::<f32, 2, 48_000>::new();
        let alternate = SimpleBuffer::<f32, 2, 48_000>::new();
        raw.push(audio(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]));
        alternate.push(audio(&[
            (10.0, 10.0),
            (20.0, 20.0),
            (30.0, 30.0),
            (40.0, 40.0),
        ]));

        let selector = make_selector(&raw, &alternate);

        let first = selector.pull(4).unwrap();
        assert_eq!(first.data(), &[1.0, 1.0, 2.0, 2.0]);

        selector.set_selected(1);
        let second = selector.pull(4).unwrap();
        assert_eq!(second.data(), &[30.0, 30.0, 40.0, 40.0]);
    }

    #[test]
    fn synchronized_select_discards_late_inactive_input_before_selecting() {
        let raw = SimpleBuffer::<f32, 2, 48_000>::new();
        let alternate = SimpleBuffer::<f32, 2, 48_000>::new();
        raw.push(audio(&[(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0)]));

        let selector = make_selector(&raw, &alternate);

        let first = selector.pull(4).unwrap();
        assert_eq!(first.data(), &[1.0, 1.0, 2.0, 2.0]);

        alternate.push(audio(&[
            (10.0, 10.0),
            (20.0, 20.0),
            (30.0, 30.0),
            (40.0, 40.0),
        ]));

        selector.set_selected(1);
        let second = selector.pull(4).unwrap();
        assert_eq!(second.data(), &[30.0, 30.0, 40.0, 40.0]);
    }
}
