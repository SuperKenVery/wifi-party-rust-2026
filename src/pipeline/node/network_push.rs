use crate::audio::frame::AudioFrame;
use crate::pipeline::node::PushNode;
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// NetworkPushNode receives AudioFrame from network and pushes AudioFrame to next node.
/// Generic over the next node type.
pub struct NetworkPushNode<Next> {
    multicast_addr: SocketAddr,
    next_node: Arc<Mutex<Next>>,
}

impl<Next> NetworkPushNode<Next>
where
    Next: PushNode<(), Input = AudioFrame, Output = AudioFrame> + Send + 'static,
{
    pub fn new(
        address: &str,
        port: u16,
        next_node: Next,
    ) -> Result<Self> {
        let multicast_ip: Ipv4Addr = address
            .parse()
            .context(format!("Failed to parse ip address: {}", address))?;

        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), port);

        Ok(Self {
            multicast_addr,
            next_node: Arc::new(Mutex::new(next_node)),
        })
    }

    pub fn start(&self) -> Result<std::thread::JoinHandle<()>> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

        socket
            .set_reuse_address(true)
            .context("Failed to set reuse_addr for receiving socket");

        let bind_addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            self.multicast_addr.port(),
        );
        socket
            .bind(&bind_addr.into())
            .context(format!("Failed to bind receive socket to {bind_addr:?}"))?;

        match self.multicast_addr.ip() {
            IpAddr::V4(ipv4_addr) => socket.join_multicast_v4(&ipv4_addr, &Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(_) => {
                anyhow::bail!("IPv6 multicast not supported");
            }
        }?;

        let socket: std::net::UdpSocket = socket.into();
        let next_node = self.next_node.clone();

        info!(
            "Network receive thread listening on multicast group {}",
            self.multicast_addr
        );

        let handle = std::thread::Builder::new()
            .name("network-receive".to_string())
            .spawn(move || {
                let mut buf = [0u8; 65536];
                loop {
                    match socket.recv_from(&mut buf) {
                        Ok((size, source_addr)) => {
                            if let Err(e) = Self::handle(&buf[..size], source_addr, &next_node) {
                                warn!("Error handling packet from {}: {:?}", source_addr, e);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to receive UDP packet: {}", e);
                        }
                    }
                }
            })
            .context("Failed to create thread to receive from network")?;

        Ok(handle)
    }

    fn handle(
        data: &[u8],
        source_addr: SocketAddr,
        next_node: &Arc<Mutex<Next>>,
    ) -> Result<()> {
        let frame = AudioFrame::deserialize(data).context("Failed to deserialize frame")?;
        anyhow::ensure!(frame.validate(), "Invalid frame from network");

        info!(
            "Received frame from {}, seq: {}",
            source_addr, frame.sequence_number
        );

        // Push AudioFrame directly without conversion
        let mut next = next_node.lock().unwrap();
        let mut null = ();
        next.push(frame, &mut null);
        Ok(())
    }
}
