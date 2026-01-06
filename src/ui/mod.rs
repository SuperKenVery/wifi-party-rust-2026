use anyhow::{Context, Result};
use std::net::UdpSocket;

use crate::state::HostId;

pub mod components;
pub use components::*;

/// Get the local IP address by creating a socket
/// This doesn't actually send any data, just queries the local routing table
pub fn get_local_ip() -> Result<HostId> {
    // Create a UDP socket and connect to a multicast address
    // This doesn't send any data, but tells us which interface would be used
    let socket = UdpSocket::bind("0.0.0.0:0").context("Failed to create socket")?;

    socket
        .connect("239.255.43.2:7667")
        .context("Failed to connect socket")?;

    let local_addr = socket.local_addr().context("Failed to get local address")?;

    Ok(HostId::from(local_addr))
}
