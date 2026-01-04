use anyhow::{Context, Result};
use rtrb::Consumer;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tracing::{error, info, warn};

use super::{MULTICAST_ADDR, MULTICAST_PORT, TTL};
use crate::state::AppState;

pub struct NetworkSender {
    socket: Socket,
    multicast_addr: SocketAddr,
}

impl NetworkSender {
    /// Create a new network sender
    pub fn new() -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;

        // Set socket to non-blocking mode for truly non-blocking UDP sends
        socket
            .set_nonblocking(true)
            .context("Failed to set non-blocking")?;

        // Set multicast TTL
        socket
            .set_multicast_ttl_v4(TTL)
            .context("Failed to set multicast TTL")?;

        // Parse multicast address
        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;

        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), MULTICAST_PORT);

        info!(
            "Network sender initialized for {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        Ok(Self {
            socket,
            multicast_addr,
        })
    }

    /// Start the send thread
    pub fn start(
        _state: Arc<AppState>,
        consumer: Consumer<Vec<u8>>,
    ) -> Result<std::thread::JoinHandle<()>> {
        let sender = Self::new()?;

        let handle = std::thread::Builder::new()
            .name("network-send".to_string())
            .spawn(move || {
                sender.run(consumer);
            })
            .context("Failed to spawn send thread")?;

        Ok(handle)
    }

    /// Run the send loop
    fn run(&self, mut consumer: Consumer<Vec<u8>>) {
        info!("Network send thread started");

        loop {
            // Pop serialized frames from the queue
            match consumer.pop() {
                Ok(serialized_frame) => {
                    // Send the packet (non-blocking)
                    match self
                        .socket
                        .send_to(&serialized_frame, &self.multicast_addr.into())
                    {
                        Ok(bytes_sent) => {
                            if bytes_sent != serialized_frame.len() {
                                warn!(
                                    "Partial send: {} of {} bytes",
                                    bytes_sent,
                                    serialized_frame.len()
                                );
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Socket buffer full, this is expected with non-blocking
                            // Just drop this frame and continue
                            warn!("Socket would block, dropping frame");
                        }
                        Err(e) => {
                            error!("Failed to send packet: {}", e);
                        }
                    }
                }
                Err(_) => {
                    // Queue empty, yield briefly to avoid busy waiting
                    std::thread::sleep(std::time::Duration::from_micros(50));
                }
            }
        }
    }
}
