//! Shared playlist for collaborative music queueing.
//!
//! [`SharedPlaylist`] maintains a replicated list of [`PlaylistEntry`] items
//! across all peers on the network. Each entry references a song by title and
//! the IP of the peer that owns the audio data. Operations (add, remove,
//! reorder, set-current) are broadcast as [`PlaylistOp`] packets and applied
//! locally on every peer.
//!
//! When an entry becomes the current entry, the owning peer starts streaming
//! it via the existing [`ShareMusicService`] pipeline. Auto-advance moves to
//! the next entry when the current song finishes (detected via party clock).

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::Duration;

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::io::NetworkSender;
use crate::party::network_stream::{NetworkStream, NetworkStreamContext};
use crate::party::tagged_packet::{PLAYLIST_TAG, PacketTag, TaggedPacket};
use crate::pipeline::Pushable;
use crate::state::{AppState, PartyViewState};

// ---------------------------------------------------------------------------
//  Entry ID generation
// ---------------------------------------------------------------------------

static NEXT_ENTRY_ID: AtomicU64 = AtomicU64::new(1);

pub fn new_entry_id() -> u64 {
    NEXT_ENTRY_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
//  Wire types
// ---------------------------------------------------------------------------

/// A single entry in the shared playlist.
///
/// `added_by` is the IP address (as string) of the peer that owns the audio
/// data. Only that peer can stream the song when it becomes current.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[rkyv(compare(PartialEq))]
pub struct PlaylistEntry {
    pub entry_id: u64,
    pub title: String,
    /// IP address (as string) of the peer that has the audio data.
    pub added_by: String,
}

/// Operations that can be applied to the shared playlist.
///
/// These are broadcast over the network and applied locally on every peer.
/// Local operations are applied immediately and then broadcast; remote
/// operations are applied on receipt. Self-echo is filtered by the
/// `PacketDispatcher` so there is no double-application.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum PlaylistOp {
    /// Append a new entry to the end of the playlist.
    Add { entry: PlaylistEntry },
    /// Remove an entry by ID.
    Remove { entry_id: u64 },
    /// Move an entry to a new position in the list.
    Move { entry_id: u64, new_index: u64 },
    /// Set the current entry (None = stop playback).
    SetCurrent { entry_id: Option<u64> },
    /// Clear the entire playlist.
    Clear,
}

// ---------------------------------------------------------------------------
//  View state (output type for GUI)
// ---------------------------------------------------------------------------

/// Snapshot of the playlist for UI consumption.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaylistState {
    pub entries: Vec<PlaylistEntry>,
    pub current_entry_id: Option<u64>,
}

// ---------------------------------------------------------------------------
//  SharedPlaylist
// ---------------------------------------------------------------------------

/// Replicated playlist state shared across all peers.
///
/// Holds the ordered list of entries, the current entry ID, and a local
/// cache of audio data for songs added by this peer. Operations are applied
/// locally and broadcast to the network.
///
/// Implements [`NetworkStream`] to receive `PLAYLIST_TAG` packets from remote
/// peers. Auto-advance is driven by a background task that checks the party
/// clock against the current song's expected end time.
pub struct SharedPlaylist {
    entries: Arc<RwLock<Vec<PlaylistEntry>>>,
    current_entry_id: Arc<RwLock<Option<u64>>>,
    /// Local cache of audio data for entries added by this peer.
    /// Keyed by entry_id. Stores (audio_bytes, title).
    local_audio_cache: Arc<DashMap<u64, (Vec<u8>, String)>>,
    /// All local IP addresses, used to determine ownership.
    local_ips: Vec<IpAddr>,
    /// IP of the interface actually used for sending. Preferred over
    /// `local_ips.first()` for the `added_by` field so that remote peers can
    /// correlate it with the source IP of our outgoing packets.
    send_ip: Option<IpAddr>,
    network_sender: NetworkSender,
    view_state: Arc<PartyViewState>,
    party_now_fn: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// Weak reference to AppState for starting playback without creating
    /// an Arc cycle (AppState -> Party -> SharedPlaylist).
    state: Weak<AppState>,
}

impl SharedPlaylist {
    pub fn new(
        state: Weak<AppState>,
        network_sender: NetworkSender,
        local_ips: Vec<IpAddr>,
        send_ip: Option<IpAddr>,
        view_state: Arc<PartyViewState>,
        party_now_fn: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            current_entry_id: Arc::new(RwLock::new(None)),
            local_audio_cache: Arc::new(DashMap::new()),
            local_ips,
            send_ip,
            network_sender,
            view_state,
            party_now_fn: Arc::new(party_now_fn),
            state,
        }
    }

    fn local_ip_string(&self) -> String {
        self.send_ip
            .or_else(|| self.local_ips.first().copied())
            .map(|ip| ip.to_string())
            .unwrap_or_default()
    }

    fn is_local_host(&self, added_by: &str) -> bool {
        self.local_ips.iter().any(|ip| ip.to_string() == added_by)
    }

    /// Start streaming the given entry if the local peer owns it.
    /// Called directly — `start_music_stream` is synchronous and spawns
    /// its own worker thread internally, just like the "Play Now" button.
    fn try_start_playback(&self, entry_id: u64) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        if let Some(entry) = self.local_audio_cache.get(&entry_id) {
            let data = entry.0.clone();
            let title = entry.1.clone();
            drop(state.start_music_stream(data, title));
        }
    }

    fn broadcast_op(&self, op: PlaylistOp) {
        let payload = match rkyv::to_bytes::<rkyv::rancor::Error>(&op) {
            Ok(bytes) => bytes.into_vec(),
            Err(e) => {
                warn!("Failed to serialize PlaylistOp: {:?}", e);
                return;
            }
        };
        self.network_sender.push(TaggedPacket {
            tag: PLAYLIST_TAG,
            payload,
        });
    }

    fn update_view_state(&self) {
        let entries = self.entries.read().unwrap().clone();
        let current = *self.current_entry_id.read().unwrap();
        self.view_state.set_playlist(entries, current);
    }

    // -- Public API (called by UI through AppState -> Party) --

    /// Add a new song to the playlist. The audio data is cached locally.
    /// If nothing is currently playing, this entry becomes current and
    /// playback starts (if the local peer is the owner, which it always is
    /// for locally-added songs).
    pub fn add_entry(&self, data: Vec<u8>, title: String) {
        let entry_id = new_entry_id();
        let entry = PlaylistEntry {
            entry_id,
            title: title.clone(),
            added_by: self.local_ip_string(),
        };

        // Cache audio data before applying op, in case apply triggers playback.
        self.local_audio_cache.insert(entry_id, (data, title));

        let op = PlaylistOp::Add { entry };
        self.apply_op(&op);
        self.broadcast_op(op);

        // If nothing is currently playing, start playing this entry.
        if self.current_entry_id.read().unwrap().is_none() {
            self.set_current(Some(entry_id));
        }
    }

    /// Remove an entry from the playlist.
    pub fn remove_entry(&self, entry_id: u64) {
        let was_current = *self.current_entry_id.read().unwrap() == Some(entry_id);

        let op = PlaylistOp::Remove { entry_id };
        self.apply_op(&op);
        self.broadcast_op(op);

        // If we removed the current song, advance to the next one.
        // Only the local initiator advances; remote peers just clear current
        // (they'll receive the subsequent SetCurrent from our advance).
        if was_current {
            self.advance_to_next();
        }
    }

    /// Move an entry to a new position in the list.
    pub fn move_entry(&self, entry_id: u64, new_index: usize) {
        // Validate locally first to avoid broadcasting an invalid op.
        {
            let entries = self.entries.read().unwrap();
            let Some(old_index) = entries.iter().position(|e| e.entry_id == entry_id) else {
                return;
            };
            if old_index == new_index || new_index >= entries.len() {
                return;
            }
        }

        let op = PlaylistOp::Move {
            entry_id,
            new_index: new_index as u64,
        };
        self.apply_op(&op);
        self.broadcast_op(op);
    }

    /// Set the current entry. `None` stops playback.
    pub fn set_current(&self, entry_id: Option<u64>) {
        let op = PlaylistOp::SetCurrent { entry_id };
        self.apply_op(&op);
        self.broadcast_op(op);
    }

    /// Clear the entire playlist.
    pub fn clear(&self) {
        let op = PlaylistOp::Clear;
        self.apply_op(&op);
        self.broadcast_op(op);
    }

    /// Skip to the next entry in the playlist.
    pub fn skip(&self) {
        self.advance_to_next();
    }

    /// Skip to the previous entry in the playlist.
    pub fn previous(&self) {
        let entries = self.entries.read().unwrap();
        let current = *self.current_entry_id.read().unwrap();
        let prev_id = current.and_then(|cur_id| {
            let idx = entries.iter().position(|e| e.entry_id == cur_id)?;
            if idx > 0 {
                Some(entries[idx - 1].entry_id)
            } else {
                None
            }
        });
        drop(entries);
        self.set_current(prev_id);
    }

    // -- Internal helpers --

    fn advance_to_next(&self) {
        let entries = self.entries.read().unwrap();
        let current = *self.current_entry_id.read().unwrap();
        let next_id = current.and_then(|cur_id| {
            let idx = entries.iter().position(|e| e.entry_id == cur_id)?;
            if idx + 1 < entries.len() {
                Some(entries[idx + 1].entry_id)
            } else {
                None
            }
        });
        drop(entries);
        self.set_current(next_id);
    }

    /// Apply an operation to local state. This is the single source of truth
    /// for state mutation — used by both local actions (via the public API
    /// above) and remote operations (via `handle`).
    fn apply_op(&self, op: &PlaylistOp) {
        match op {
            PlaylistOp::Add { entry } => {
                self.entries.write().unwrap().push(entry.clone());
            }
            PlaylistOp::Remove { entry_id } => {
                self.entries
                    .write()
                    .unwrap()
                    .retain(|e| e.entry_id != *entry_id);
                self.local_audio_cache.remove(entry_id);
                if *self.current_entry_id.read().unwrap() == Some(*entry_id) {
                    *self.current_entry_id.write().unwrap() = None;
                }
            }
            PlaylistOp::Move {
                entry_id,
                new_index,
            } => {
                let mut entries = self.entries.write().unwrap();
                if let Some(old_index) = entries.iter().position(|e| e.entry_id == *entry_id) {
                    let new_index = *new_index as usize;
                    if old_index != new_index && new_index < entries.len() {
                        let entry = entries.remove(old_index);
                        entries.insert(new_index, entry);
                    }
                }
            }
            PlaylistOp::SetCurrent { entry_id } => {
                *self.current_entry_id.write().unwrap() = *entry_id;
                // If the local peer owns this entry, start streaming now.
                if let Some(entry_id) = entry_id {
                    let entries = self.entries.read().unwrap();
                    if let Some(entry) = entries.iter().find(|e| e.entry_id == *entry_id) {
                        let owns = self.is_local_host(&entry.added_by);
                        drop(entries);
                        if owns {
                            self.try_start_playback(*entry_id);
                        }
                    }
                }
            }
            PlaylistOp::Clear => {
                self.entries.write().unwrap().clear();
                self.local_audio_cache.clear();
                *self.current_entry_id.write().unwrap() = None;
            }
        }
        self.update_view_state();
    }

    /// Check if the current song has finished and auto-advance if so.
    ///
    /// Called by a background task. Uses the party clock to determine
    /// completion: if `party_now >= start_party_time + duration`, the song
    /// is finished.
    pub fn check_auto_advance(&self, state: &AppState) {
        let current_id = *self.current_entry_id.read().unwrap();
        let Some(current_id) = current_id else {
            return;
        };

        // Only the owner of the current song auto-advances.
        let entries = self.entries.read().unwrap();
        let Some(entry) = entries.iter().find(|e| e.entry_id == current_id) else {
            return;
        };
        if !self.is_local_host(&entry.added_by) {
            return;
        }
        drop(entries);

        // Find the local sender stream to check progress.
        let streams = state.view_state.synced_streams();
        let Some(stream) = streams.iter().find(|s| s.is_local_sender) else {
            return;
        };

        // Need total_samples and start_party_time to compute end time.
        let total_samples = stream.meta.total_samples;
        if total_samples == 0 {
            return;
        }
        let start_party_time = stream.progress.start_party_time;
        if start_party_time == 0 {
            return;
        }
        if !stream.progress.is_playing {
            return;
        }

        let sample_rate = stream.meta.codec_params.sample_rate;
        if sample_rate == 0 {
            return;
        }

        let duration_us = total_samples * 1_000_000 / sample_rate as u64;
        let end_party_time = start_party_time.saturating_add(duration_us);
        let party_now = (self.party_now_fn)();

        if party_now >= end_party_time {
            info!(
                "Playlist: auto-advancing (party_now={} >= end={})",
                party_now, end_party_time
            );
            self.advance_to_next();
        }
    }

    /// Start a background task that monitors playback and auto-advances.
    pub fn start_auto_advance_task(self: &Arc<Self>, state: Arc<AppState>) {
        let playlist = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            loop {
                interval.tick().await;
                playlist.check_auto_advance(&state);
            }
        });
    }
}

impl<S: AudioSample, const C: usize, const SR: u32> NetworkStream<S, C, SR> for SharedPlaylist {
    fn tags(&self) -> &'static [PacketTag] {
        &[PLAYLIST_TAG]
    }

    fn handle(&self, _source: SocketAddr, _tag: PacketTag, bytes: &[u8]) -> anyhow::Result<()> {
        let op = rkyv::from_bytes::<PlaylistOp, rkyv::rancor::Error>(bytes)
            .map_err(|e| anyhow::anyhow!("PlaylistOp deserialize: {:?}", e))?;
        self.apply_op(&op);
        Ok(())
    }

    fn start(self: Arc<Self>, _ctx: NetworkStreamContext) {
        // Start the auto-advance task.
        if let Some(state) = self.state.upgrade() {
            self.start_auto_advance_task(state);
        }
    }
}
