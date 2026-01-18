//! Configuration for Party audio/network devices.

use cpal::DeviceId;
use std::net::Ipv4Addr;

/// Configuration for Party audio and network devices.
///
/// `None` values mean use system default (for audio) or all interfaces (for network).
#[derive(Clone, Default, Debug)]
pub struct PartyConfig {
    pub input_device_id: Option<DeviceId>,
    pub output_device_id: Option<DeviceId>,
    pub send_interface_ip: Option<Ipv4Addr>,
}
