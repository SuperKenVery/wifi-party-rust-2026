//! UDP multicast network I/O.
//!
//! This module provides:
//! - Socket creation and configuration for UDP multicast
//! - [`NetworkSender`] for broadcasting audio packets to all peers
//!
//! # Multicast Configuration
//!
//! IPv4:
//! - Address: `239.255.43.2` (administratively scoped multicast)
//! - Port: `7667`
//! - TTL: `1` (local network only)
//!
//! IPv6:
//! - Address: `ff02::7667` (link-local scope multicast)
//! - Port: `7667`
//! - Hop limit: `1` (local network only)

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::Arc;

use anyhow::{Context, Result};
use network_interface::NetworkInterfaceConfig;
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{error, info, warn};

use crate::party::realtime_stream::NetworkPacket;
use crate::pipeline::Pushable;

pub const MULTICAST_ADDR_V4: &str = "239.255.43.2";
pub const MULTICAST_ADDR_V6: &str = "ff02::7667";
pub const MULTICAST_PORT: u16 = 7667;
pub const TTL: u32 = 1;

const DSCP_EF: u32 = 0xB8;

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

fn allow_awdl(socket: &Socket, allow: bool) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use libc::{SOL_SOCKET, c_int, setsockopt};
        use std::os::unix::io::AsRawFd;

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

/// Creates an IPv4 multicast socket ready for sending and receiving.
///
/// Returns the socket, multicast address, and list of local IPs (for filtering own packets).
pub fn create_multicast_socket_v4(
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
        .set_nonblocking(true)
        .context("Failed to set nonblocking")?;
    socket
        .set_multicast_ttl_v4(TTL)
        .context("Failed to set multicast_ttl_v4")?;
    socket
        .set_multicast_loop_v4(false)
        .context("Failed to set multicast_loop_v4")?;
    set_socket_dscp(&socket, false);
    let _ = allow_awdl(&socket, true);

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

/// Creates an IPv6 multicast socket ready for sending and receiving.
///
/// Returns the socket, multicast address, and list of local IPs (for filtering own packets).
pub fn create_multicast_socket_v6(
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
        .set_nonblocking(true)
        .context("Failed to set nonblocking")?;
    socket
        .set_multicast_hops_v6(TTL)
        .context("Failed to set multicast_hops_v6")?;
    socket
        .set_multicast_loop_v6(false)
        .context("Failed to set multicast_loop_v6")?;
    set_socket_dscp(&socket, true);
    let _ = allow_awdl(&socket, true);

    if let Some(index) = send_interface_index {
        socket
            .set_multicast_if_v6(index)
            .context("Failed to set multicast_if_v6")?;
        info!("Send interface set to index {}", index);
    }

    let bind_addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, MULTICAST_PORT, 0, 0);
    socket
        .bind(&bind_addr.into())
        .context(format!("Failed to bind to {:?}", bind_addr))?;

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

/// Creates a multicast socket based on the IPv6 flag.
pub fn create_multicast_socket(
    ipv6: bool,
    send_interface_index: Option<u32>,
) -> Result<(UdpSocket, SocketAddr, Vec<IpAddr>)> {
    if ipv6 {
        create_multicast_socket_v6(send_interface_index)
    } else {
        create_multicast_socket_v4(send_interface_index)
    }
}

/// Sends audio packets to all peers via UDP multicast.
///
/// Implements [`Pushable`] so it can be used directly in the audio pipeline.
/// Packets are serialized with `rkyv` and broadcast to the multicast group.
///
/// # Cloning
///
/// `NetworkSender` can be cloned to share the same socket between multiple
/// audio pipelines (e.g., mic and system audio).
#[derive(Clone)]
pub struct NetworkSender {
    socket: Arc<UdpSocket>,
    multicast_addr: SocketAddr,
}

impl NetworkSender {
    pub fn new(socket: UdpSocket, multicast_addr: SocketAddr) -> Self {
        info!("Network sender initialized for {:?}", multicast_addr);

        Self {
            socket: Arc::new(socket),
            multicast_addr,
        }
    }

    fn send_packet(&self, packet: &NetworkPacket) {
        if let Err(error) = self.send_inner(packet) {
            error!("{}", error);
        }
    }

    fn send_inner(&self, packet: &NetworkPacket) -> Result<()> {
        let serialized =
            rkyv::to_bytes::<rkyv::rancor::Error>(packet).context("Failed to serialize packet")?;

        let addr = self.multicast_addr;
        let sent_length = self
            .socket
            .send_to(&serialized, addr)
            .context(format!("Failed to send packet to {addr:?}"))?;

        if sent_length < serialized.len() {
            warn!("Partial sent: {}/{}", sent_length, serialized.len());
        }

        Ok(())
    }
}

impl Pushable<NetworkPacket> for NetworkSender {
    fn push(&self, input: NetworkPacket) {
        self.send_packet(&input);
    }
}
