//! Switch effect for conditionally passing or blocking audio.

use crate::audio::frame::AudioBuffer;
use crate::pipeline::Node;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Conditionally passes or blocks audio based on an AtomicBool flag.
/// Passes audio when flag is true, blocks when false.
/// When it's disabled, downstream get no data at all, not even silence data.
#[derive(Clone)]
pub struct Switch<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    enabled: Arc<AtomicBool>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Switch<Sample, CHANNELS, SAMPLE_RATE> {
    pub fn new(enabled: Arc<AtomicBool>) -> Self {
        Self {
            enabled,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample: Send + Sync, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Switch<Sample, CHANNELS, SAMPLE_RATE>
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
