use crate::audio::frame::AudioFrame;
use crate::audio::AudioSample;
use crate::network::receive::{HostPipelineManager, NetworkReceiver, NetworkSource};
use crate::network::send::NetworkSender;
use crate::pipeline::{Sink, Source};
use crate::state::AppState;
use anyhow::Result;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::error;

pub struct NetworkNode<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    _receiver_handle: Option<thread::JoinHandle<()>>,
    _marker: PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            _receiver_handle: None,
            _marker: PhantomData,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>:
        for<'a> rkyv::Serialize<rkyv::api::high::HighSerializer<rkyv::util::AlignedVec, rkyv::ser::allocator::ArenaHandle<'a>, rkyv::rancor::Error>>,
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: rkyv::Archive,
    <AudioFrame<Sample, CHANNELS, SAMPLE_RATE> as rkyv::Archive>::Archived:
        rkyv::Deserialize<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    pub fn start(
        &mut self,
        pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
        state: Arc<AppState>,
    ) -> Result<(
        impl Sink<Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>>,
        impl Source<Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>>,
    )> {
        let sender = NetworkSender::<Sample, CHANNELS, SAMPLE_RATE>::new()?;

        let pipeline_manager_clone = pipeline_manager.clone();
        let state_clone = state.clone();
        let receiver_handle = thread::spawn(move || {
            match NetworkReceiver::<Sample, CHANNELS, SAMPLE_RATE>::new(
                state_clone,
                pipeline_manager_clone,
            ) {
                Ok(receiver) => receiver.run(),
                Err(e) => error!("Failed to start network receiver: {}", e),
            }
        });

        self._receiver_handle = Some(receiver_handle);

        let source = NetworkSource::new(pipeline_manager);

        Ok((sender, source))
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}
