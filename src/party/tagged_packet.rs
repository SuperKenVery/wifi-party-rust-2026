//! Wire envelope for all network packets.
//!
//! Every packet sent over UDP is wrapped in a [`TaggedPacket`] that carries
//! a [`PacketTag`] identifying the payload type and the raw rkyv-serialized
//! payload bytes. The receiver looks up the tag in [`StreamRegistry`] and
//! hands off the payload bytes to the matching [`NetworkStream`].
//!
//! Adding a new packet type only requires:
//! 1. Define a tag constant here.
//! 2. Define the payload struct with rkyv derives in the stream module.
//! 3. Implement [`NetworkStream`] on the stream and register it in `Party::run`.

use rkyv::{Archive, Deserialize, Serialize};

pub type PacketTag = u32;

/// Top-level wire envelope — the only type serialized directly to UDP.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct TaggedPacket {
    pub tag: PacketTag,
    pub payload: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Tag constants — one per payload type.
// ---------------------------------------------------------------------------

pub const REALTIME_TAG: PacketTag = 1;
pub const SYNCED_TAG: PacketTag = 2;
pub const SYNCED_META_TAG: PacketTag = 3;
pub const SYNCED_CONTROL_TAG: PacketTag = 4;
pub const REQUEST_FRAMES_TAG: PacketTag = 5;
pub const NTP_TAG: PacketTag = 6;
