# Wi-Fi Party

I want to build a piece of software that can turn a home into a KTV. Two usage scenarios:

1. One computer connected to speakers; everyone uses their phones as microphones.
2. A group of people wearing headphones and connected via their phones, singing together.

In either case, what this software needs to do is:

1. Send microphone audio over the network. We only transmit within the local network, using multicast, 242.355.43.2.
2. Mix the audio received from the multicast group and play it back.

## Requirements

1. Written in Rust
2. Use dioxus as the GUI library
3. Use nix flakes to manage dependencies and builds. Although the final goal is cross-platform support, due to limited disk space we will initially only build and test the macOS version.
4. Low-latency audio is extremely important.
5. Don't use unsafe rust. Thread safety is why I abandonded the cpp version and rewrite in rust.

### Audio Latency

Audio latency is critical in a karaoke scenario.

1. We must use low-latency native system APIs. That is:
   - macOS/iOS: AudioUnit
   - Windows: WASAPI
   - Android: AAudio
   - etc.

   Therefore, we use the `cpal` library to interact with system audio.

2. We should minimize network latency as much as possible. Thus, we should use the `rkyv` library for zero-copy serialization/deserialization.
3. We also need to optimize end-to-end pipeline latency. We should minimize cloning, especially clones of audio data buffers, which should be avoided as much as possible.
4. We should also focus on multithread lock efficiency.
   - What we are doing is basically receive -> mix -> play, and record -> send.
   - For both part, with an audio and network thread,

## Testing

- We should use [sansio](https://github.com/webrtc-rs/sansio) so that we could test our code easily.

## External Libraries

We should use existing library when possible, which provides better feature/performance and less maintainence burden.

Below, I give some libraries I found useful. You could use them, or you could find something even better.

- SPSC queues: [rtrb](https://docs.rs/rtrb/latest/rtrb/)
- Jitter buffer: [neteq](https://github.com/security-union/videocall-rs/tree/main/neteq)
- Make testing easier: [sansio](https://github.com/webrtc-rs/sansio)
- (De)serialization: [rkyv](https://docs.rs/rkyv)

## Logics

You actually need quite some logic to receive the audio from network:

- Receive for each host
- Jitter buffer to deal with out-of-order packets
- Mix audio from all hosts
- Put it into a ring buffer for the audio thread to take

We also send microphone audio and/or system audio to the network. For microphone audio, we would want a loopback so that user knows how he/she is singing. (But we absolutely don't want this for system audio)

And some things we may implement but not for now. You don't need to implement these, but you should make sure the architecture makes them easy:

- Use some method to extract the instrumental part from system audio
- Echo elimination for microphone
- ...
