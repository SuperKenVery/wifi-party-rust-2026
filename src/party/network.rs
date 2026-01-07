use crate::network::{receive::NetworkReceiver, send::NetworkSender};
use crate::state::AppState;
use anyhow::Result;
use rtrb::{Producer, RingBuffer};
use std::sync::Arc;
use std::thread;
use tracing::error;

pub struct NetworkNode {
    _sender_handle: Option<thread::JoinHandle<()>>,
    _receiver_handle: Option<thread::JoinHandle<()>>,
}

impl NetworkNode {
    pub fn new() -> Self {
        Self {
            _sender_handle: None,
            _receiver_handle: None,
        }
    }

    /// Starts the network threads and returns the Producer for the sender queue.
    pub fn start(&mut self, state: Arc<AppState>) -> Result<Producer<Vec<u8>>> {
        let (producer, consumer) = RingBuffer::<Vec<u8>>::new(500);
        
        // Start Network Sender Thread
        let state_clone_send = state.clone();
        let sender_handle = thread::spawn(move || {
            if let Err(e) = NetworkSender::start(state_clone_send, consumer) {
                error!("Failed to start network sender: {}", e);
            }
        });

        // Start Network Receiver Thread
        let state_clone_recv = state.clone();
        let receiver_handle = thread::spawn(move || {
            if let Err(e) = NetworkReceiver::start(state_clone_recv) {
                error!("Failed to start network receiver: {}", e);
            }
        });

        self._sender_handle = Some(sender_handle);
        self._receiver_handle = Some(receiver_handle);

        Ok(producer)
    }
}
