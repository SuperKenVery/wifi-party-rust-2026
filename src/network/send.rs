use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::{info, warn};

use super::{MULTICAST_ADDR, MULTICAST_PORT, TTL};
use crate::audio::AudioFrame;
use crate::pipeline::Sink;

pub struct NetworkSender {
    socket: Socket,
    multicast_addr: SocketAddr,
}

impl NetworkSender {
    pub fn new() -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;

        socket
            .set_nonblocking(true)
            .context("Failed to set non-blocking")?;

        socket
            .set_multicast_ttl_v4(TTL)
            .context("Failed to set multicast TTL")?;

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

    fn send_frame(&self, frame: &AudioFrame) {
        match frame.serialize() {
            Ok(serialized) => {
                match self.socket.send_to(&serialized, &self.multicast_addr.into()) {
                    Ok(bytes_sent) => {
                        if bytes_sent != serialized.len() {
                            warn!(
                                "Partial send: {} of {} bytes",
                                bytes_sent,
                                serialized.len()
                            );
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        warn!("Socket would block, dropping frame");
                    }
                    Err(e) => {
                        warn!("Failed to send packet: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to serialize frame: {}", e);
            }
        }
    }
}

impl Sink for NetworkSender {
    type Input = AudioFrame;

    fn push(&self, input: Self::Input) {
        self.send_frame(&input);
    }
}
