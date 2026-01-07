use crate::audio::frame::{AudioBuffer, AudioFrame};
use crate::audio::AudioSample;
use crate::pipeline::node::PushNode;
use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tracing::{info, warn};

pub struct NetworkPushNode<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample: AudioSample> {
    multicast_addr: SocketAddr,
    next_node: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample: AudioSample>
    NetworkPushNode<CHANNELS, SAMPLE_RATE, Sample>
{
    pub fn new(
        address: &str,
        port: u16,
        next_node: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
    ) -> Result<Self> {
        let multicast_ip: Ipv4Addr = address
            .parse()
            .context(format!("Failed to parse ip address: {}", address))?;

        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), port);

        Ok(Self {
            multicast_addr,
            next_node,
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
            IpAddr::V6(ipv6_addr) => socket.join_multicast_v6(&ipv6_addr, 0),
        }?;

        let socket: std::net::UdpSocket = socket.into();

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
                            if let Err(e) = Self::handle(&buf[..size], source_addr) {
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

    fn handle(&mut self, data: &[u8], source_addr: SocketAddr) -> Result<()> {
        let frame = AudioFrame::deserialize(data).context("Failed to deserialize frame")?;
        anyhow::ensure!(frame.validate(), "Invalid frame from network");

        info!(
            "Received frame from {}, seq: {}",
            source_addr, frame.sequence_number
        );

        self.next_node.push(frame, next);
        Ok(())
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next, Sample: AudioSample>
    PushNode<CHANNELS, SAMPLE_RATE, Sample, Next> for NetworkPushNode<CHANNELS, SAMPLE_RATE, Sample>
where
    Next: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
{
    fn push(&mut self, frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, next: &mut Next) {
        next.push(frame, &mut ());
    }
}
