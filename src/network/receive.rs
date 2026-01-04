use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::{MULTICAST_ADDR, MULTICAST_PORT};
use crate::audio::AudioFrame;
use crate::state::{AppState, ConnectionStatus, HostId, HostInfo};

pub struct NetworkReceiver {
    socket: std::net::UdpSocket,
}

impl NetworkReceiver {
    /// Create a new network receiver and join multicast group
    pub fn new() -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;

        // Allow address reuse
        socket
            .set_reuse_address(true)
            .context("Failed to set reuse address")?;

        // Bind to multicast port on all interfaces
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .context("Failed to bind socket")?;

        // Join multicast group
        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;

        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .context("Failed to join multicast group")?;

        info!(
            "Network receiver joined multicast group {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        Ok(Self {
            socket: socket.into(),
        })
    }

    /// Create a [`NetworkReceiver`] and start the receive thread
    pub fn start(state: Arc<AppState>) -> Result<std::thread::JoinHandle<()>> {
        let receiver = Self::new()?;

        // Update connection status
        *state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let handle = std::thread::Builder::new()
            .name("network-receive".to_string())
            .spawn(move || {
                receiver.run(state);
            })
            .context("Failed to spawn receive thread")?;

        Ok(handle)
    }

    /// Run the receive loop
    fn run(&self, state: Arc<AppState>) {
        info!("Network receive thread started");

        let mut buf = [0u8; 65536];

        loop {
            if let Err(e) = self.handle_packet(&mut buf, &state) {
                warn!("Error processing packet: {:?}", e);
            }
        }
    }

    /// Handle a single incoming packet
    fn handle_packet(&self, buf: &mut [u8], state: &Arc<AppState>) -> Result<()> {
        let (size, source_addr) = self
            .socket
            .recv_from(buf)
            .context("Failed to receive UDP packet")?;

        let received_data = &buf[..size];

        let host_id = HostId::from(source_addr);

        let frame = AudioFrame::deserialize(received_data)
            .context(format!("Failed to deserialize frame from {}", source_addr))?;

        if !frame.validate() {
            anyhow::bail!("Invalid frame from {}", source_addr);
        }

        // Check if this is our own packet (loopback)
        {
            let local_host_id = state.local_host_id.lock().unwrap();
            if let Some(local_id) = *local_host_id {
                if host_id == local_id {
                    return Ok(());
                }
            }
        }
        debug!(
            "Reiceive packet from {:?}, seq num {}, is_v4: {}",
            source_addr,
            frame.sequence_number,
            source_addr.is_ipv4()
        );

        // Update host tracking and push to jitter buffer
        {
            let mut hosts = state.active_hosts.lock().unwrap();
            hosts
                .entry(host_id)
                .and_modify(|info| {
                    info.last_seen = std::time::Instant::now();
                })
                .or_insert_with(|| {
                    info!("New host detected: {}", host_id.to_string());
                    HostInfo::new(host_id)
                });
        }

        // Create jitter buffer for new host if needed
        state.jitter_buffers.get_or_create(host_id);

        // Push frame to jitter buffer
        state.jitter_buffers.push_frame(host_id, frame);

        Ok(())
    }
}
