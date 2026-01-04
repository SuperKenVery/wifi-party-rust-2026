# Initialize Wi-Fi Party Core Architecture (Final Sans-IO)

This plan implements a **Sans-IO Audio Graph** using `refpool` for zero-copy memory management. The `AudioEngine` API is split to handle network events and audio frames independently, reflecting their asynchronous nature.

## 1. Project Configuration

**File:** `Cargo.toml`

* **Action:** Add dependencies.

* **Dependencies:**

  * `refpool`: Efficient object pooling.

  * `cpal`: Audio I/O.

  * `rkyv`: Zero-copy serialization.

  * `rtrb`: Lock-free ring buffer.

  * `tokio`: Async runtime.

  * `socket2`: Multicast UDP.

  * `anyhow`, `thiserror`: Error handling.

  * `tracing`: Logging.

  * `bytes`: Buffer management.

  * `rand`: Peer IDs.

## 2. Memory Management

**File:** `src/logic/memory.rs`

* **Types:**

  * `pub type AudioBuffer = Vec<f32>;`

  * `pub type BufferHandle = refpool::PoolRef<AudioBuffer>;`

* **Pool:** `refpool::Pool` used to allocate `BufferHandle`s.

## 3. Audio Graph Logic (Pure Sans-IO) (`src/logic/`)

### A. The Node Interface

**File:** `src/logic/graph/node.rs`

* **Trait:** `AudioNode`

  * `fn process(&mut self, inputs: Vec<BufferHandle>, pool: &Pool) -> Vec<BufferHandle>;`

### B. The Engine (The Facade)

**File:** `src/logic/engine.rs`

* **Struct:** `AudioEngine`

  * `graph`: `AudioGraph`

  * `mic_node`: NodeId

  * `speaker_node`: NodeId

  * `jitter_nodes`: `HashMap<PeerId, NodeId>`

* **API (Sans-IO):**

  * `fn handle_network_packet(&mut self, packet: AudioPacket)`

    * **Logic:** Identifies the peer, finds the corresponding `JitterBufferNode`, and pushes the packet into the node's internal queue. This is a pure state update (no audio processing).

  * `fn process_audio_frame(&mut self, mic_data: &[f32]) -> (Vec<f32>, Vec<AudioPacket>)`

    * **Logic:**

      1.  **Inject:** Copy `mic_data` into a `BufferHandle` and feed `mic_node`.

      2.  **Run Cycle:** Execute the graph BFS. `JitterBufferNode`s will dequeue packets and produce audio. `MixerNode` will sum them.

      3.  **Extract:** Get result from `speaker_node` for playback.

      4.  **Packetize:** Collect any output intended for the network (from the graph's "Network Output" nodes if we add them, or simply by encoding the mic input if that's the separate path).

## 4. Standard Nodes

**File:** `src/logic/nodes.rs`

* **MixerNode:** Sums inputs.

* **JitterBufferNode:** 0 Inputs. Stores packets in `handle_network_packet`. Outputs audio in `process`.

* **GainNode:** 1 Input -> 1 Output.

## 5. I/O Shells (The "Dirty" Layer)

### A. Network Layer (`src/network.rs`)

* **Loop:** `socket.recv()` -> `engine_lock.handle_network_packet()`.

### B. Audio Layer (`src/audio.rs`)

* **Loop:** `mic_stream` -> `engine_lock.process_audio_frame()` -> `speaker_stream`.

## 6. Integration (`src/main.rs`)

* Initialize Engine -> Wrap in `Arc<Mutex<>>` (or use channels to communicate with the actor) -> Run I/O.

