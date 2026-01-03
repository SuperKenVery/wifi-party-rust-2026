<br />

## 1. Project Initialization & Dependencies

* Update `Cargo.toml` with:

  * `cpal` (Audio I/O)

  * `rkyv` (Zero-copy serialization)

  * `rtrb` (SPSC Ring Buffer)

  * `tokio` (Async Runtime)

  * `socket2` (Advanced socket configuration)

  * `anyhow` (Error handling)

## 2. Architecture: The `sansio` Pattern

I will separate the **logic** (state changes, mixing, packetizing) from the **IO** (reading/writing sockets and sound cards).

* **`src/protocol.rs`**: Define `AudioPacket` with `rkyv`.

* **`src/logic.rs`** **(The** **`sansio`** **core)**:

  * Pure functions/structs that handle:

    * `handle_audio_input(samples) -> Option<Packet>`

    * `handle_network_packet(packet) -> AudioBuffer`

  * This module will be fully testable without a network or audio device.

* **`src/network.rs`** **(IO Shell)**:

  * Uses `socket2` to manage the UDP socket.

  * Feeds data into `logic.rs` components.

* **`src/audio.rs`** **(IO Shell)**:

  * Manages `cpal` streams.

  * Feeds audio data into `logic.rs` components.

## 3. Implementation Steps

1. **Dependencies**: Update `Cargo.toml`.
2. **Protocol & Logic**: Implement the core `AudioPacket` and the processing logic (mixing/buffering) in a pure, testable way (`src/logic.rs`).
3. **Network IO**: Implement the multicast socket setup using `socket2`, integrating with the logic layer.
4. **Audio IO**: Implement `cpal` streams that drive the logic layer.
5. **Integration**: Wire everything together in `main.rs`.

## 4. Documentation

* Update `AGENTS.md` with:

  * **`sansio`** **pattern**: Explanation of how to extend the logic without touching IO.

  * **`rkyv`**: Usage examples.

  * **`socket2`**: Multicast setup snippets.

