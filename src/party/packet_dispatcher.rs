//! Network packet receiving and dispatching.
//!
//! [`PacketDispatcher`] runs a background task that:
//! 1. Receives UDP datagrams from the multicast socket.
//! 2. Ignores packets sent by this process (self-echo).
//! 3. Passes each datagram to [`StreamRegistry::dispatch`], which
//!    deserializes the [`TaggedPacket`] envelope and routes the payload
//!    to the matching [`NetworkStream`].

use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::audio::AudioSample;
use crate::party::network_stream::StreamRegistry;
use crate::state::{AppState, ConnectionStatus};

pub struct PacketDispatcher;

impl PacketDispatcher {
    pub fn start<S: AudioSample, const C: usize, const SR: u32>(
        socket: UdpSocket,
        local_ips: Vec<IpAddr>,
        state: Arc<AppState>,
        registry: Arc<StreamRegistry<S, C, SR>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self::run(socket, local_ips, state, registry).await;
        })
    }

    async fn run<S: AudioSample, const C: usize, const SR: u32>(
        socket: UdpSocket,
        local_ips: Vec<IpAddr>,
        state: Arc<AppState>,
        registry: Arc<StreamRegistry<S, C, SR>>,
    ) {
        info!("Packet dispatcher started, local IPs: {:?}", local_ips);

        let socket = Arc::new(
            tokio::net::UdpSocket::from_std(socket).expect("Failed to convert to tokio UdpSocket"),
        );

        *state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let mut buf = [0u8; 65536];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((size, source_addr)) => {
                    if local_ips.contains(&source_addr.ip()) {
                        continue;
                    }
                    if let Err(e) = registry.dispatch(source_addr, &buf[..size]) {
                        error!("Packet handling error: {:?}", e);
                    }
                }
                Err(e) => error!("Failed to receive UDP packet: {:?}", e),
            }
        }
    }
}
