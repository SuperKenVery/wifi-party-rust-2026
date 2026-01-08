use crate::audio::AudioFrame;
use crate::network::receive::{HostPipelineManager, NetworkReceiver, NetworkSource};
use crate::network::send::NetworkSender;
use crate::pipeline::{Sink, Source};
use crate::state::AppState;
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::error;

pub struct NetworkNode {
    pipeline_manager: Arc<Mutex<HostPipelineManager>>,
    _receiver_handle: Option<thread::JoinHandle<()>>,
}

impl NetworkNode {
    pub fn new() -> Self {
        Self {
            pipeline_manager: Arc::new(Mutex::new(HostPipelineManager::new())),
            _receiver_handle: None,
        }
    }

    pub fn start(
        &mut self,
        state: Arc<AppState>,
    ) -> Result<(impl Sink<Input = AudioFrame>, impl Source<Output = AudioFrame>)> {
        let sender = NetworkSender::new()?;

        let pipeline_manager = self.pipeline_manager.clone();
        let receiver_handle = thread::spawn(move || {
            match NetworkReceiver::new(state, pipeline_manager) {
                Ok(receiver) => receiver.run(),
                Err(e) => error!("Failed to start network receiver: {}", e),
            }
        });

        self._receiver_handle = Some(receiver_handle);

        let source = NetworkSource::new(self.pipeline_manager.clone());

        Ok((sender, source))
    }
}

impl Default for NetworkNode {
    fn default() -> Self {
        Self::new()
    }
}
