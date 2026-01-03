use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use crossbeam_channel::Sender;
use tracing::{info, warn, error};

use crate::audio::AudioFrame;
use crate::state::{AppState, HostId, HostInfo, ConnectionStatus};
use super::{MULTICAST_ADDR, MULTICAST_PORT};

pub struct NetworkReceiver {
    socket: Socket,
}

impl NetworkReceiver {
    /// Create a new network receiver and join multicast group
    pub fn new() -> Result<Self, String> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| format!("Failed to create socket: {}", e))?;

        // Allow address reuse
        socket
            .set_reuse_address(true)
            .map_err(|e| format!("Failed to set reuse address: {}", e))?;

        // Bind to multicast port on all interfaces
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .map_err(|e| format!("Failed to bind socket: {}", e))?;

        // Join multicast group
        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .map_err(|e| format!("Invalid multicast address: {}", e))?;
        
        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .map_err(|e| format!("Failed to join multicast group: {}", e))?;

        info!("Network receiver joined multicast group {}:{}", MULTICAST_ADDR, MULTICAST_PORT);

        Ok(Self { socket })
    }

    /// Start the receive thread
    pub fn start(
        state: Arc<AppState>,
        frame_sender: Sender<(HostId, AudioFrame)>,
    ) -> Result<std::thread::JoinHandle<()>, String> {
        let receiver = Self::new()?;
        
        // Update connection status
        *state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let handle = std::thread::Builder::new()
            .name("network-receive".to_string())
            .spawn(move || {
                receiver.run(state, frame_sender);
            })
            .map_err(|e| format!("Failed to spawn receive thread: {}", e))?;

        Ok(handle)
    }

    /// Run the receive loop
    fn run(&self, state: Arc<AppState>, frame_sender: Sender<(HostId, AudioFrame)>) {
        info!("Network receive thread started");

        let mut buf = [std::mem::MaybeUninit::<u8>::uninit(); 65536];

        loop {
            // Receive packet
            match self.socket.recv_from(&mut buf) {
                Ok((size, source_addr)) => {
                    // Safety: recv_from initializes the buffer up to size
                    let received_data = unsafe {
                        std::slice::from_raw_parts(buf.as_ptr() as *const u8, size)
                    };
                    // Extract source IP
                    let source_ip = match source_addr.as_socket() {
                        Some(SocketAddr::V4(addr)) => addr.ip().octets(),
                        _ => {
                            warn!("Received packet from non-IPv4 source");
                            continue;
                        }
                    };

                    // Deserialize frame
                    match AudioFrame::deserialize(received_data) {
                        Ok(frame) => {
                            // Validate frame
                            if !frame.validate() {
                                warn!("Invalid frame from {:?}", source_ip);
                                continue;
                            }

                            // Create HostId from source IP (extracted from UDP packet)
                            let host_id = HostId::from(source_ip);

                            // Check if this is our own packet (loopback)
                            let local_host_id = state.local_host_id.lock().unwrap();
                            if let Some(local_id) = *local_host_id {
                                if host_id == local_id {
                                    // This is our own packet, skip it (unless loopback is enabled)
                                    // For now, skip to avoid feedback
                                    continue;
                                }
                            }

                            // Update host tracking
                            {
                                let mut hosts = state.active_hosts.lock().unwrap();
                                hosts.entry(host_id)
                                    .and_modify(|info| {
                                        info.last_seen = std::time::Instant::now();
                                    })
                                    .or_insert_with(|| {
                                        info!("New host detected: {}", host_id.to_string());
                                        HostInfo::new(host_id)
                                    });
                            }

                            // Send frame with host_id to mixer via channel
                            if frame_sender.send((host_id, frame)).is_err() {
                                error!("Failed to send frame to mixer, channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize frame from {:?}: {}", source_ip, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to receive packet: {}", e);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }

        info!("Network receive thread stopped");
    }
}
