//! Gain (volume) effect.

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::graph::{PipelineGraph, Inspectable};
use crate::pipeline::Node;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Applies a gain (volume multiplier) to all samples.
///
/// # Example
///
/// ```ignore
/// let gain = Gain::<f32, 2, 48000>::new(0.5); // 50% volume
/// let pipeline = source.pipe(gain);
/// ```
#[derive(Debug, Clone)]
pub struct Gain<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    factor: Sample,
    // Store stats as atomic bits (f64 bits)
    stats: Arc<AtomicU64>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Gain<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(factor: Sample) -> Self {
        Self {
            factor,
            stats: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Inspectable
    for Gain<Sample, CHANNELS, SAMPLE_RATE>
{
    fn get_visual(&self, graph: &mut PipelineGraph) -> String {
        let id = format!("{:p}", self);
        
        let val_bits = self.stats.load(Ordering::Relaxed);
        let val = f64::from_bits(val_bits);
        let width = (val * 100.0).clamp(0.0, 100.0);
        let color = if val > 0.9 { "#EF4444" } else { "#10B981" };
        
        let svg = format!(
            r#"<div class="w-full h-full bg-green-900 border border-green-600 rounded flex flex-col items-center justify-center shadow-lg p-2">
                <div class="text-xs font-bold text-green-200 mb-1">Gain</div>
                <div class="w-full h-2 bg-gray-700 rounded-full overflow-hidden">
                    <div class="h-full transition-all duration-100" style="width: {}%; background-color: {}"></div>
                </div>
            </div>"#,
            width, color
        );
        
        graph.add_node(id.clone(), svg);
        id
    }
}

impl<Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Gain<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, mut input: Self::Input) -> Option<Self::Output> {
        let center = Sample::silence();
        let mut max_val = 0.0;
        
        for sample in input.data_mut() {
            *sample = (*sample - center) * self.factor + center;
            let val = sample.to_f64_normalized().abs();
            if val > max_val {
                max_val = val;
            }
        }
        
        // Update stats (simple peak meter)
        // Note: For smoother display, we might want to decay instead of instant update,
        // but this is fine for a demo.
        let current = f64::from_bits(self.stats.load(Ordering::Relaxed));
        let new_val = if max_val > current {
             max_val 
        } else {
             current * 0.9 // Decay
        };
        self.stats.store(new_val.to_bits(), Ordering::Relaxed);
        
        Some(input)
    }
}
