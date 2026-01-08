pub mod audio;
pub mod network;

use crate::state::AppState;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use self::audio::{AudioInput, AudioOutput};
use self::network::NetworkNode;

pub struct Party {
    state: Arc<AppState>,
    network_node: NetworkNode,
}

impl Party {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            network_node: NetworkNode::new(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        info!("Starting Party pipelines...");

        todo!()
    }
}
