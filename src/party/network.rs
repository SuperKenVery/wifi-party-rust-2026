//! Network node orchestration.
//!
//! This module provides [`NetworkNode`], which coordinates the network layer for
//! audio transport. It manages both sending (via [`NetworkSender`]) and receiving
//! (via [`NetworkReceiver`]) of audio packets over UDP multicast.
//!
//! # Architecture
//!
//! ```text
//! Local Audio Input
//!       │
//!       ▼
//! ┌───────────────────┐
//! │OpusEncoder        │
//! │  + FramePacker    │
//! └─────────┬─────────┘
//!           │
//!           ▼
//! ┌─────────────┐
//! │NetworkSender│ ──── UDP Multicast ────► Other Peers
//! └─────────────┘
//!
//!                                          Other Peers
//!                                               │
//!                                          UDP Multicast
//!                                               │
//!                                               ▼
//!                                     ┌───────────────────┐
//!                                     │NetworkReceiver    │
//!                                     │(background thread)│
//!                                     └────────┬──────────┘
//!                                              │
//!                                              ▼
//!                                   ┌──────────────────────┐
//!                                   │RealtimeAudioStream   │
//!                                   │(OpusDecoder +        │
//!                                   │ jitter buffers)      │
//!                                   └──────────┬───────────┘
//!                                              │
//!                                              ▼
//!                                       Local Speaker
//! ```
//!
//! # Usage
//!
//! Call [`NetworkNode::start`] to initialize network transport. It returns:
//! - A [`Sink`] for sending [`NetworkPacket`]s (Opus-encoded) to the network
//! - A reference to [`RealtimeAudioStream`] that provides mixed audio from all peers

use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use network_interface::NetworkInterfaceConfig;
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::io::{
    MULTICAST_ADDR_V4, MULTICAST_ADDR_V6, MULTICAST_PORT, NetworkReceiver, NetworkSender, TTL,
};
use crate::party::ntp::NtpService;
use crate::party::stream::RealtimeAudioStream;
use crate::party::sync_stream::SyncedAudioStreamManager;
use crate::state::AppState;

const DSCP_EF: u32 = 0xB8;

/// Sets the **DSCP (Differentiated Services Code Point)** value on a socket for QoS.
///
/// Uses EF (Expedited Forwarding) = 46, which translates to TOS/Traffic Class = 0xB8.
/// This marks packets as voice traffic for network QoS prioritization.
fn set_socket_dscp(socket: &Socket, ipv6: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = socket.as_raw_fd();
        let dscp = DSCP_EF as libc::c_int;

        let (level, optname) = if ipv6 {
            (libc::IPPROTO_IPV6, libc::IPV6_TCLASS)
        } else {
            (libc::IPPROTO_IP, libc::IP_TOS)
        };

        let ret = unsafe {
            libc::setsockopt(
                fd,
                level,
                optname,
                &dscp as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        if ret != 0 {
            warn!(
                "Failed to set DSCP for voice QoS: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::{
            IP_TOS, IPPROTO_IP, IPPROTO_IPV6, IPV6_TCLASS, setsockopt,
        };

        let fd = socket.as_raw_socket();
        let dscp = DSCP_EF as i32;

        let (level, optname) = if ipv6 {
            (IPPROTO_IPV6, IPV6_TCLASS)
        } else {
            (IPPROTO_IP, IP_TOS)
        };

        let ret = unsafe {
            setsockopt(
                fd as usize,
                level,
                optname,
                &dscp as *const _ as *const i8,
                std::mem::size_of::<i32>() as i32,
            )
        };
        if ret != 0 {
            warn!(
                "Failed to set DSCP for voice QoS: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

/// Allow sending and receiving via **Apple Wireless Direct Link**.
///
/// Reference: https://github.com/seemoo-lab/proxawdl?tab=readme-ov-file
///
/// For packets to actually be sent, we would need a way to trigger in OS e.g. open an AirDrop sharing panel or registering some type of NetworkService. However, it turns out that AWDL has a very high packet loss (~90%) and aggressive batching in our usecase, and `llw0` behaves the same, even when disconnecting Wi-Fi. Therefore, I won't implement opening a NetworkService until we find a way to make awdl/llw really send my packets with full effort.
fn allow_awdl(socket: &Socket, allow: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use libc::{SOL_SOCKET, c_int, setsockopt};
        use std::os::unix::io::AsRawFd;

        // XNU socket option for unrestricted inbound processing
        // https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/sys/socket_private.h#L228
        const SO_RECV_ANYIF: c_int = 0x1104;

        let fd = socket.as_raw_fd();
        let value: c_int = if allow { 1 } else { 0 };
        let ret = unsafe {
            setsockopt(
                fd,
                SOL_SOCKET,
                SO_RECV_ANYIF,
                &value as *const _ as *const libc::c_void,
                std::mem::size_of::<c_int>() as libc::socklen_t,
            )
        };
        if ret != 0 {
            Err(std::io::Error::last_os_error().into())
        } else {
            Ok(())
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = socket;
        let _ = allow;
        Ok(())
    }
}

/// Orchestrates network audio transport.
///
/// `NetworkNode` manages the lifecycle of network sender and receiver components,
/// providing a simple interface for the audio pipeline to send and receive packets.
///
/// # Thread Model
///
/// - The sender operates synchronously when packets are pushed
/// - The receiver runs in a dedicated background thread, continuously listening
///   for incoming packets and dispatching them to stream handlers
pub struct NetworkNode<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    receiver_handle: Option<thread::JoinHandle<()>>,
    shutdown_flag: Arc<AtomicBool>,
    _marker: PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            receiver_handle: None,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            _marker: PhantomData,
        }
    }

    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.receiver_handle.take() {
            let _ = handle.join();
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Drop
    for NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    /// Starts the network transport layer.
    ///
    /// This initializes the UDP multicast sender and spawns a background thread
    /// for the receiver. Supports both IPv4 and IPv6.
    ///
    /// # Arguments
    ///
    /// * `ipv6` - If true, use IPv6 multicast, otherwise IPv4.
    /// * `send_interface_index` - Interface index to send multicast packets on.
    ///   If None, uses the system default.
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `NetworkSender` - Push [`NetworkPacket`]s here to broadcast to other peers
    /// - `Arc<RealtimeAudioStream>` - Pull from here to get mixed audio from all peers.
    /// - `Arc<SyncedAudioStream>` - Pull from here to get synced music audio.
    /// - `Arc<NtpService>` - The NTP service for time synchronization.
    pub fn start(
        &mut self,
        realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        state: Arc<AppState>,
        ipv6: bool,
        send_interface_index: Option<u32>,
    ) -> Result<(
        NetworkSender,
        Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
        Arc<NtpService>,
    )> {
        let (socket, multicast_addr, local_ips) = if ipv6 {
            Self::setup_socket_v6(send_interface_index)?
        } else {
            Self::setup_socket_v4(send_interface_index)?
        };

        let send_socket = socket
            .try_clone()
            .context("Failed to clone socket for sender")?;
        let sender = NetworkSender::new(send_socket, multicast_addr);

        let ntp_service = NtpService::new(sender.clone(), self.shutdown_flag.clone());

        let ntp_for_synced = ntp_service.clone();
        let synced_stream = Arc::new(SyncedAudioStreamManager::new(move || {
            ntp_for_synced.party_now()
        }));

        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .context("Failed to set socket read timeout")?;

        let realtime_stream_clone = realtime_stream.clone();
        let synced_stream_clone = synced_stream.clone();
        let ntp_service_clone = ntp_service.clone();
        let state_clone = state.clone();
        let sender_clone = sender.clone();
        let shutdown_flag = self.shutdown_flag.clone();
        let receiver_handle = thread::spawn(move || {
            let receiver = NetworkReceiver::<Sample, CHANNELS, SAMPLE_RATE>::new(
                socket,
                state_clone,
                sender_clone,
                realtime_stream_clone,
                synced_stream_clone,
                ntp_service_clone,
                local_ips,
                shutdown_flag,
            );
            receiver.run();
        });

        self.receiver_handle = Some(receiver_handle);
        Ok((sender, realtime_stream, synced_stream, ntp_service))
    }

    fn setup_socket_v4(
        send_interface_index: Option<u32>,
    ) -> Result<(UdpSocket, SocketAddr, Vec<IpAddr>)> {
        let multicast_ip: Ipv4Addr = MULTICAST_ADDR_V4
            .parse()
            .context("Invalid multicast address")?;
        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), MULTICAST_PORT);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;
        socket
            .set_reuse_address(true)
            .context("Failed to set reuse_address")?;
        socket
            .set_nonblocking(false)
            .context("Failed to set nonblocking")?;
        socket
            .set_multicast_ttl_v4(TTL)
            .context("Failed to set multicast_ttl_v4")?;
        socket
            .set_multicast_loop_v4(false)
            .context("Failed to set multicast_loop_v4")?;
        set_socket_dscp(&socket, false);
        allow_awdl(&socket, true);

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .context(format!("Failed to bind to {:?}", bind_addr))?;

        let mut local_ips = Vec::new();
        let mut send_ip: Option<Ipv4Addr> = None;

        match network_interface::NetworkInterface::show() {
            Ok(interfaces) => {
                for iface in interfaces {
                    for addr in &iface.addr {
                        if let IpAddr::V4(ip) = addr.ip() {
                            if ip.is_loopback() {
                                continue;
                            }
                            local_ips.push(IpAddr::V4(ip));
                            if send_interface_index == Some(iface.index) {
                                send_ip = Some(ip);
                            }
                            match socket.join_multicast_v4(&multicast_ip, &ip) {
                                Ok(()) => info!("Joined multicast on {} ({})", iface.name, ip),
                                Err(e) => warn!(
                                    "Failed to join multicast on {} ({}): {}",
                                    iface.name, ip, e
                                ),
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to enumerate interfaces: {:?}, using default", e);
                socket.join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)?;
            }
        }

        if let Some(ip) = send_ip {
            socket.set_multicast_if_v4(&ip)?;
            info!("Send interface set to {}", ip);
        }

        info!(
            "IPv4 multicast socket ready on {}:{}",
            MULTICAST_ADDR_V4, MULTICAST_PORT
        );
        Ok((socket.into(), multicast_addr, local_ips))
    }

    fn setup_socket_v6(
        send_interface_index: Option<u32>,
    ) -> Result<(UdpSocket, SocketAddr, Vec<IpAddr>)> {
        let multicast_ip: Ipv6Addr = MULTICAST_ADDR_V6
            .parse()
            .context("Invalid IPv6 multicast address")?;
        let multicast_addr = SocketAddr::V6(SocketAddrV6::new(multicast_ip, MULTICAST_PORT, 0, 0));

        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create IPv6 socket")?;
        socket
            .set_reuse_address(true)
            .context("Failed to set reuse_address")?;
        socket
            .set_nonblocking(false)
            .context("Failed to set nonblocking")?;
        socket
            .set_multicast_hops_v6(TTL)
            .context("Failed to set multicast_hops_v6")?;
        socket
            .set_multicast_loop_v6(false)
            .context("Failed to set multicast_loop_v6")?;
        set_socket_dscp(&socket, true);
        allow_awdl(&socket, true);

        if let Some(index) = send_interface_index {
            socket
                .set_multicast_if_v6(index)
                .context("Failed to set multicast_if_v6")?;
            info!("Send interface set to index {}", index);
        }

        let bind_addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, MULTICAST_PORT, 0, 0);
        socket
            .bind(&bind_addr.into())
            .context(format!("Failed to bind tor {:?}", bind_addr))?;

        let mut local_ips = Vec::new();
        match network_interface::NetworkInterface::show() {
            Ok(interfaces) => {
                for iface in interfaces {
                    for addr in &iface.addr {
                        if let IpAddr::V6(ip) = addr.ip() {
                            if ip.is_loopback() {
                                continue;
                            }
                            local_ips.push(IpAddr::V6(ip));
                            match socket.join_multicast_v6(&multicast_ip, iface.index) {
                                Ok(()) => info!("Joined IPv6 multicast on {} ({})", iface.name, ip),
                                Err(e) => warn!(
                                    "Failed to join IPv6 multicast on {} ({}): {}",
                                    iface.name, ip, e
                                ),
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to enumerate interfaces: {:?}, using default", e);
                socket.join_multicast_v6(&multicast_ip, 0)?;
            }
        }

        info!(
            "IPv6 multicast socket ready on [{}]:{}",
            MULTICAST_ADDR_V6, MULTICAST_PORT
        );
        Ok((socket.into(), multicast_addr, local_ips))
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}
