# 🎤 Wi-Fi Party

**Turn your living room into a karaoke room**

Ever wanted to have a KTV party at home? You don't need expensive equipment — your phones and computers already have everything you need! Just connect to the same Wi-Fi, launch the app, and start singing together.

Wi-Fi Party KTV lets everyone on your local network share audio in real-time. Grab a mic, play some music, and let the party begin.

## ✨ What You Can Do

- **🎙️ Share Your Mic** — Sing into your phone or laptop, **everyone hears** you instantly
- **🔊 Share System Audio** — Playing a backing track? Share it with the room
- **🎵 Synchronized Music** — Stream music files that play in perfect sync across all devices
- **👥 Everyone Joins** — No setup, no accounts — just connect to the same network

## 🎯 Why This Exists

Commercial KTV systems cost thousands. Bluetooth speakers have annoying latency. Screen mirroring is clunky. 

What if your existing devices could just... talk to each other? That's Wi-Fi Party KTV — a peer-to-peer audio sharing app that turns any local network into a karaoke room, with latency low enough to actually sing along.

## ⚡ Built for Low Latency

Real-time audio is hard. We obsessed over every millisecond:

- **Lock-free queues** — No mutex contention in the audio path
- **Minimal audio buffers** — As small as cpal allows
- **Adaptive jitter buffering** — Smooth playback without adding delay
- **Zero-copy serialization** — rkyv for network packets
- **DSCP/QoS marking** — Network priority for audio traffic

## 🛠️ Tech Stack

| Component | Technology |
|-----------|------------|
| UI | [Dioxus](https://dioxuslabs.com/) Desktop |
| Audio | [cpal](https://github.com/RustAudio/cpal) + [Opus](https://opus-codec.org/) |
| Music Decoding | [Symphonia](https://github.com/pdeljanov/Symphonia) (MP3, FLAC, OGG, WAV, AAC) |
| Network | UDP Multicast |
| Serialization | [rkyv](https://github.com/rkyv/rkyv) |

## 🚀 Quick Start

Download from [Releases](#) or build from source (see [HACKING.md](HACKING.md)).

Launch the app on each device. They'll automatically discover each other on the local network.

## 🏗️ How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                         Party (Orchestrator)                    │
├─────────────────────────────────────────────────────────────────┤
│  Mic Pipeline          System Audio Pipeline    Music Pipeline  │
│  ┌──────────────┐      ┌──────────────┐        ┌──────────────┐ │
│  │ AudioInput   │      │ LoopbackInput│        │ MusicStreamer│ │
│  │ → LevelMeter │      │ → LevelMeter │        │ → Symphonia  │ │
│  │ → Gain       │      │ → Switch     │        │ → NTP Sync   │ │
│  │ → Switch     │      │ → Batcher    │        └──────────────┘ │
│  │ → Tee ───────│      │ → Opus       │                         │
│  │   ↓ Loopback │      │ → Network    │                         │
│  │   ↓ Network  │      └──────────────┘                         │
│  └──────────────┘                                               │
├─────────────────────────────────────────────────────────────────┤
│                    Network Layer (UDP Multicast)                │
│         IPv4: 239.255.43.2:7667  │  IPv6: ff02::7667:7667       │
├─────────────────────────────────────────────────────────────────┤
│  Receive Pipeline                                               │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │ NetworkReceiver → JitterBuffer → OpusDecoder → Mixer → Out  ││
│  │                 → SyncedStream (NTP-scheduled playback) ↗   ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## 📄 License

GPLv3
