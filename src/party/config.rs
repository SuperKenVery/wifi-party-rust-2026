//! Configuration for Party audio/network devices.

use cpal::DeviceId;

#[derive(Clone, Default, Debug)]
pub struct PartyConfig {
    pub input_device_id: Option<DeviceId>,
    pub output_device_id: Option<DeviceId>,
    pub ipv6: bool,
    pub send_interface_index: Option<u32>,
}
