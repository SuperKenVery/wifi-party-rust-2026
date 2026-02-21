# Wi-Fi Party

Real-time audio sharing application using UDP multicast on local networks.

## What It Does

- Records mic or system audio, encodes with Opus, sends via UDP multicast
- Receives audio from network peers, decodes, buffers, mixes, plays to speaker
- Supports synchronized music playback with NTP-based time sync

## Key Engineering Challenge

Reducing latency. Techniques used:
- Lock-free queues (atomic cells in JitterBuffer)
- Low-level cpal API with minimal buffer sizes (3ms)
- Adaptive jitter buffering
- Static pipeline dispatching (zero-overhead)
- DSCP/QoS marking for network priority

## Module Overview

### `src/audio/` - Audio Data & Processing

Core audio types and processing nodes.

Files:
- `sample.rs` - `AudioSample` trait for sample types (f32, i16)
- `frame.rs` - `AudioBuffer` (raw PCM) and `AudioFrame` (buffer + sequence number)
- `opus.rs` - Opus encoder/decoder with FEC (used for realtime mic/system audio)
- `symphonia_compat.rs` - Symphonia compatibility layer: `WireCodecType`, `WireCodecParams` for network serialization

Subdirectories:
- `buffers/` - Buffer implementations
  - `simple_buffer.rs` - Basic FIFO buffer
  - `audio_batcher.rs` - Batches samples to reduce packet frequency
  - `jitter_buffer.rs` - Reorders packets, adaptive latency, lock-free design
- `effects/` - Audio effects
  - `gain.rs` - Volume control
  - `level_meter.rs` - Audio level metering
  - `noise_gate.rs` - RMS-based noise gate
  - `switch.rs` - Enable/disable audio stream

### `src/io/` - Hardware & Network I/O

Files:
- `audio.rs` - cpal-based audio device access
  - `AudioInput` - Microphone capture
  - `LoopbackInput` - System audio capture
  - `AudioOutput` - Speaker playback
- `network.rs` - UDP multicast networking
  - `create_multicast_socket_v4/v6` - Socket factory with DSCP/QoS, AWDL support
  - `NetworkSender` - Broadcasts packets to multicast group
  - Constants: `MULTICAST_ADDR_V4/V6`, `MULTICAST_PORT`, `TTL`

### `src/party/` - Orchestration

The main coordination layer that wires everything together.

Files:
- `party.rs` - Main `Party` struct that sets up all pipelines and manages component lifecycle
- `config.rs` - Configuration (device IDs, network settings)
- `packet_dispatcher.rs` - `PacketDispatcher` receives UDP packets and dispatches by type
- `stream.rs` - `RealtimeAudioStream` manages per-host jitter buffers
- `sync_stream.rs` - `SyncedAudioStreamManager` for synchronized music playback
- `ntp.rs` - NTP service for time synchronization
- `music.rs` - Music file streaming
- `combinator.rs` - Pipeline utilities (Tee, Mixer)

### `src/pipeline/` - Processing Framework

Dynamic pipeline architecture for data flow with runtime graph modification.

Files:
- `traits.rs` - Core `Node` trait for data transformation
  - `Node` - Transforms input to output (has associated types `Input`/`Output`)
- `dyn_traits.rs` - Object-safe dynamic traits and pipeline macros
  - `Pullable<T>` - Can return data when pulled (object-safe)
  - `Pushable<T>` - Can receive pushed data (object-safe)
  - `push_chain!` macro - Build push-based pipelines declaratively
  - `pull_chain!` macro - Build pull-based pipelines declaratively
- `graph_node.rs` - `GraphNode<N>` wrapper to make any Node implement `Pushable`/`Pullable`

### `src/state/` - Application State

Single file `mod.rs` containing:
- `AppState` - Global state (volumes, enabled flags, host list)
- `HostId` - Remote peer identifier (IP-based)
- `HostInfo` - Peer metadata and stream info
- `MusicStreamProgress` - Music encoding/streaming progress

### `src/ui/` - User Interface

Dioxus-based desktop UI.

Files:
- `app.rs` - Main app component
- `sidebar.rs` - Navigation sidebar
- `sidebar_panels/` - Panel components
  - `audio_control.rs` - Mic/system audio controls
  - `participants.rs` - Connected peers list
  - `share_music.rs` - Music upload/control
  - `debug.rs` - Debug info display

## Data Flow

### Sending (Mic Pipeline)

Built with `push_chain!` macro:
```
AudioInput -> push_chain![
    LevelMeter,
    Gain,
    => Tee(
        push_chain![AudioBatcher, OpusEncoder, RealtimeFramePacker, => NetworkSender],
        push_chain![Switch (loopback), => SimpleBuffer]
    )
]
```

### Sending (System Audio Pipeline)

Built with `push_chain!` macro:
```
LoopbackInput -> push_chain![
    LevelMeter,
    Switch (system_enabled),
    AudioBatcher,
    OpusEncoder,
    RealtimeFramePacker,
    => NetworkSender
]
```

### Receiving

1. `PacketDispatcher` runs in background thread, receives UDP packets
2. Packets dispatched by type:
   - `Realtime` → `RealtimeAudioStream` → per-host `DecodeChain` → `Mixer`
   - `Synced` → `SyncedAudioStreamManager` (NTP-synchronized playback)
   - `Ntp` → `NtpService`
3. Per-host DecodeChain (created dynamically on first packet):
   - `GraphNode<RealtimeFrameDecoder>` → `JitterBuffer` → registered with `Mixer`
4. Output mixer built with `Mixer::with_inputs()`:
   - `pull_chain![realtime_stream.mixer() =>, Switch]` - network voice/system audio
   - `pull_chain![synced_stream =>, Switch]` - network music
   - `loopback_buffer` - local mic loopback (no switch)
5. `AudioOutput` plays mixed audio via cpal callback

## Network Protocol

Multicast addresses:
- IPv4: `239.255.43.2:7667`, TTL=1
- IPv6: `ff02::7667:7667`, hop_limit=1

Packet types (serialized with rkyv):
- `Realtime` - Opus audio for realtime streams (mic/system)
- `Synced` - Original codec audio for synchronized music (pass-through, no re-encoding)
- `SyncedMeta` - Music stream metadata (includes `WireCodecParams` for receiver decoding)
- `SyncedControl` - Play/pause/seek commands
- `RequestFrames` - Retransmission requests
- `Ntp` - Time sync messages

## Platform-Specific Notes

### Android

Android requires `WifiManager.MulticastLock` to receive multicast UDP packets. Without it, the OS filters out multicast traffic to save battery.

Files:
- `assets/AndroidManifest.xml` - Declares `CHANGE_WIFI_MULTICAST_STATE` and `ACCESS_WIFI_STATE` permissions
- `src/io/multicast_lock.rs` - JNI wrapper that acquires/releases MulticastLock via WifiManager

The lock is acquired in `Party::run()` and held for the lifetime of the network connection.

## Key Components Detail

### JitterBuffer (`src/audio/buffers/jitter_buffer.rs`)

Slot-based buffer indexed by sequence number. Key behaviors:
- On push: clamp read_seq forward if outside target latency window
- On pull: only hold back on underrun (read_seq > write_seq)
- Adaptive latency: increases on high loss (>5%), decreases when stable
- Uses `AtomicCell` and `AtomicU64` for lock-free read/write

### RealtimeAudioStream (`src/party/stream.rs`)

Manages per-host decode chains using the dynamic pipeline architecture:
- `DecodeChain`: `GraphNode<RealtimeFrameDecoder>` → `JitterBuffer` → `Mixer`
- Creates chain on first packet from new source (dynamic host management)
- Internal `Mixer` combines audio from all per-host JitterBuffers
- Implements `Source` trait - `pull` delegates to internal mixer
- On cleanup timeout: removes chain from DashMap and deregisters from mixer
- Has `start_cleanup_task()` for background stale host cleanup

### SyncedAudioStreamManager (`src/party/sync_stream.rs`)

For synchronized music playback. Uses NTP time to schedule frame playback. Supports retransmission requests for missing frames.

Key design: No re-encoding. Raw compressed packets from the original audio file are forwarded over the network. Receiver creates a symphonia decoder based on `WireCodecParams` from metadata.

Has `start_cleanup_task()` and `start_retransmit_task()` for background operations.

### Party (`src/party/party.rs`)

Main orchestrator. `Party::run()` sets up all pipelines:
1. Creates multicast socket via `create_multicast_socket()`
2. Creates `NetworkSender` for packet transmission
3. Creates `NtpService`, `SyncedAudioStreamManager` with their background tasks
4. Spawns network thread with `PacketDispatcher` for receiving
5. Creates mic and system audio pipelines
6. Creates output pipeline with Mixer
7. Starts host sync task (updates UI with active hosts)

## Entry Point

`src/main.rs`:
1. Initialize logger
2. Create `PartyConfig::default()`
3. Create `AppState::new(config)` which internally creates and runs `Party`
4. Launch Dioxus desktop app with `AppState` as context
