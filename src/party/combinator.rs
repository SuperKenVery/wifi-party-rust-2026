//! Pipeline combinators for audio routing.
//!
//! Provides utilities for splitting, switching, and mixing audio streams.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::graph::{PipelineGraph, Inspectable};
use crate::pipeline::{Node, Sink, Source};

#[derive(Clone)]
pub struct Tee<A, B> {
    a: A,
    b: B,
}

impl<A, B> Tee<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: Inspectable, B: Inspectable> Inspectable for Tee<A, B> {
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        let svg = format!(
            r#"<div class="w-full h-full bg-yellow-900 border border-yellow-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-yellow-200">Tee</div>
            </div>"#
        );
        graph.add_node(id.clone(), svg);
        
        let a_id = self.a.get_visual(graph);
        let b_id = self.b.get_visual(graph);
        
        graph.add_edge(id.clone(), a_id, None);
        graph.add_edge(id.clone(), b_id, None);
        
        id
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

#[derive(Clone)]
pub struct LoopbackSwitch<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    enabled: Arc<AtomicBool>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    LoopbackSwitch<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(enabled: Arc<AtomicBool>) -> Self {
        Self {
            enabled,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Inspectable
    for LoopbackSwitch<Sample, CHANNELS, SAMPLE_RATE>
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        let active = self.enabled.load(std::sync::atomic::Ordering::Relaxed);
        let status = if active { "ON" } else { "OFF" };
        let color = if active { "#10B981" } else { "#EF4444" };
        
        let svg = format!(
            r#"<div class="w-full h-full bg-purple-900 border border-purple-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-purple-200 mb-1">Loopback</div>
                <div class="text-xs font-bold" style="color: {}">{}</div>
            </div>"#,
            color, status
        );
        graph.add_node(id.clone(), svg);
        id
    }
}

impl<Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for LoopbackSwitch<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        if self.enabled.load(Ordering::Acquire) {
            Some(input)
        } else {
            None
        }
    }
}

#[derive(Clone)]
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

impl<A: Inspectable, B: Inspectable, Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Inspectable
    for MixingSource<A, B, Sample, CHANNELS, SAMPLE_RATE>
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        let svg = format!(
            r#"<div class="w-full h-full bg-yellow-900 border border-yellow-600 rounded flex flex-col items-center justify-center shadow-lg">
                <div class="text-xs font-bold text-yellow-200">Mixer</div>
            </div>"#
        );
        graph.add_node(id.clone(), svg);
        
        let a_id = self.a.get_visual(graph);
        let b_id = self.b.get_visual(graph);
        
        graph.add_edge(a_id, id.clone(), None);
        graph.add_edge(b_id, id.clone(), None);
        
        id
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
