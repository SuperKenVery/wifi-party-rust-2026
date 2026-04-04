---
name: share-synchronized-music
description: Understand and modify the synced music sharing pipeline in Wi-Fi Party, including the Dioxus share music UI, provider flow, Party handoff, raw compressed packet streaming, NTP-scheduled playback, retransmission, and pause/resume/seek behavior.
---

Use this skill when working on the synced music feature exposed by `src/ui/sidebar_panels/share_music.rs`.

## Scope

This feature is distinct from realtime mic/system audio:
- Realtime audio is re-encoded to Opus and played ASAP with jitter buffering.
- Synced music forwards original compressed packets from the file, decodes them on receivers, and schedules playback against shared party time.

Primary files:
- `src/ui/sidebar_panels/share_music.rs`
- `src/music_provider/mod.rs`
- `src/music_provider/local_file.rs`
- `src/state/mod.rs`
- `src/party/party.rs`
- `src/party/music.rs`
- `src/party/sync_stream.rs`
- `src/party/packet_dispatcher.rs`
- `src/audio/symphonia_compat.rs`
- `src/ui/app.rs`

## End-to-end flow

1. UI entry
- `ShareMusicPanel` renders available `MusicProvider`s.
- Current provider set comes from `AppState.music_provider_factories`.
- The active stream list shown in the panel is polled from `AppState::synced_stream_states()` by `src/ui/app.rs` every 100 ms.

2. Provider handoff
- `LocalFileProvider` reads bytes from the chosen file and calls `AppState::start_music_stream(data, file_name)`.

3. Party handoff
- `AppState` forwards to `Party::start_music_stream(...)`.
- `Party` passes shared runtime pieces into `MusicStream::start(...)`:
  - `NetworkSender`
  - `NtpService`
  - `SyncedAudioStreamManager`
  - `MusicStreamProgress`

4. Sender startup
- `MusicStream::start(...)` opens the file with Symphonia.
- It converts Symphonia codec parameters into `WireCodecParams`.
- It creates a new `stream_id`.
- It sends:
  - `NetworkPacket::SyncedMeta`
  - `NetworkPacket::SyncedControl(Start { party_clock_time, seq: 1 })`
- It also injects those same messages locally into `SyncedAudioStreamManager` using the loopback `127.0.0.1:0` sender identity.

5. Sender worker thread
- `StreamContext::run()` loops while active.
- It reads compressed packets from the file into a sender-side `vault: DashMap<u64, RawPacket>` keyed by sequence number.
- It sends `SyncedFrame` packets from that vault ahead of playback.
- It updates `MusicStreamProgress` for UI display.

6. Network receive path
- `PacketDispatcher` deserializes `NetworkPacket` and routes:
  - `Synced` -> `SyncedAudioStreamManager::receive`
  - `SyncedMeta` -> `receive_meta`
  - `SyncedControl` -> `receive_control`
  - `RequestFrames` -> `Party::handle_retransmission_request`

7. Receiver decode/buffer path
- `SyncedAudioStreamManager` creates one `BufferEntry` per `(source_addr, stream_id)` when metadata arrives.
- `BufferEntry` owns:
  - Symphonia decoder
  - optional rubato resampler
  - `pending_raw` for out-of-order compressed frames
  - `decoded_frames` for decoded PCM frames
- Frames must be decoded in sequence order because compressed codecs are stateful.

8. Playback path
- `SyncedAudioStreamManager` implements `Pullable<AudioBuffer<...>>`.
- `Party::run()` mixes synced music into the output mixer alongside realtime audio and local loopback.
- `pull_and_mix()` computes playback position from party time:
  - if `party_now < start_party_time`, do not play
  - otherwise convert elapsed microseconds to elapsed samples
  - find the decoded frame containing that sample position
  - copy samples into the local mix buffer

## Protocol and control model

Metadata:
- `SyncedStreamMeta` carries `stream_id`, file name, estimated/exact totals, and `WireCodecParams`.

Audio data:
- `SyncedFrame` carries raw compressed bytes from the source file plus `dur` and `sequence_number`.

Controls:
- `SyncedControl::Start { stream_id, party_clock_time, seq }`
- `SyncedControl::Pause { stream_id }`

Behavior:
- Start/resume is implemented by sending a new scheduled `Start`.
- Pause flips playback off without deleting buffered frames.
- Seek is also implemented as a new scheduled `Start` at a different sequence number.

## Important design choices

### No re-encoding

Synced music does not go through the Opus realtime path.
- Sender reads compressed packets directly from the source file.
- Receiver decodes with the original codec using `WireCodecParams`.
- This preserves exact packet timing/model better for synchronized playback and avoids extra encode/decode loss.

### Streaming faster than realtime

In `src/party/music.rs`:
- `SEND_RATE_MULTIPLIER = 2`
- `REDUNDANCY_COUNT = 2`

Meaning:
- Sender pushes buffered compressed packets faster than playback consumes them.
- Each packet is multicast twice.
- This reduces the chance that receivers stall before playback time.

### Retransmission

Receiver side:
- `start_retransmit_task()` periodically calls `get_missing_frames()`.
- Missing sequence numbers are sent as `NetworkPacket::RequestFrames`.

Sender side:
- `Party` looks up the matching local `MusicStream` by `stream_id`.
- `MusicStream` queues retransmit requests.
- `send_retransmissions()` resends requested packets from `vault`.

## Pause, resume, seek

Pause:
- `MusicStream::handle_pause()` sends `Pause`.
- It computes `last_pause_seq` from current party time relative to the last start point.

Resume:
- `handle_resume()` sends a future `Start` using `last_pause_seq`.

Seek:
- `handle_seek(pos_ms)` converts milliseconds to target samples, then to sequence number.
- It sends a future `Start` at that sequence.
- If seeking beyond already-read packets, it also seeks the Symphonia format reader and resumes packet ingestion from there.

## Progress and UI interpretation

Sender UI:
- `MusicStreamProgress` tracks file name, encoding state, frames read, and frames sent.
- `ShareMusicPanel` uses this for the sender progress bars.

Receiver UI:
- `SyncedAudioStreamManager::active_streams()` reports:
  - `buffered_frames`
  - `highest_seq_received`
  - `samples_played`
  - `is_playing`
- `ShareMusicPanel` uses that for receiver progress bars and transport controls.

## Invariants to preserve

- Decode compressed frames strictly in sequence order.
- Do not route synced music through the realtime Opus pipeline.
- `SyncedControl::Start` semantics are absolute party-time scheduling, not immediate local playback.
- `WireCodecParams` must remain sufficient to recreate the receiver decoder.
- Local sender preview depends on injecting metadata/control/data into the same `SyncedAudioStreamManager` using loopback `127.0.0.1:0`.
- Output mixing depends on `SyncedAudioStreamManager` staying `Pullable`.

## Current constraints and caveats

- Only one synced music stream is effectively supported at a time. `receive_meta()` clears old buffers when a different `stream_id` appears.
- Early `total_frames` is estimated from duration and corrected later at EOF.
- `get_missing_frames()` scans a bounded range and uses decoded-frame presence as its loss signal.
- Playback position calculation depends on contiguous decoded frames from `start_seq` onward; gaps stop local fill.

## Change checklist

When editing this feature, verify:
- file selection/provider still calls `AppState::start_music_stream`
- sender still emits `SyncedMeta` before or with `Start`
- receiver can still construct a decoder from `WireCodecParams`
- out-of-order arrival still buffers raw frames until decode order is safe
- retransmit requests still reach the owning local `MusicStream`
- pause/resume/seek still emit future scheduled `Start` control when appropriate
- synced stream still appears in the `Party` output mixer
- UI polling in `src/ui/app.rs` still surfaces active synced streams

## Fast orientation

Read in this order for most tasks:
1. `src/ui/sidebar_panels/share_music.rs`
2. `src/music_provider/local_file.rs`
3. `src/state/mod.rs`
4. `src/party/party.rs`
5. `src/party/music.rs`
6. `src/party/sync_stream.rs`
7. `src/party/packet_dispatcher.rs`
8. `src/audio/symphonia_compat.rs`
