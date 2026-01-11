use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::{info, warn};

use super::{MULTICAST_ADDR, MULTICAST_PORT, TTL};
use crate::audio::frame::AudioFrame;
use crate::audio::AudioSample;
use crate::pipeline::Sink;

pub struct NetworkSender<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    socket: Socket,
    multicast_addr: SocketAddr,
    _marker: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
{
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
            _marker: PhantomData,
        })
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>:
        for<'a> rkyv::Serialize<rkyv::api::high::HighSerializer<rkyv::util::AlignedVec, rkyv::ser::allocator::ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    fn send_frame(&self, frame: &AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        match rkyv::to_bytes::<rkyv::rancor::Error>(frame) {
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

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>:
        for<'a> rkyv::Serialize<rkyv::api::high::HighSerializer<rkyv::util::AlignedVec, rkyv::ser::allocator::ArenaHandle<'a>, rkyv::rancor::Error>>,
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: Self::Input) {
        self.send_frame(&input);
    }
}
