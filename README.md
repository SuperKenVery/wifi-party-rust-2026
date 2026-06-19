**English** | [中文](README.zh.md)

# 🎤 Wi-Fi Party

**Turn your living room into a karaoke room**

Ever wanted to have a KTV party at home? You don't need expensive equipment — your phones and computers already have everything you need! Just connect to the same Wi-Fi, launch the app, and start singing together.

Wi-Fi Party lets everyone on your local network share audio in real-time. Grab a mic, play some music, and let the party begin.

## ✨ What You Can Do

- **🎙️ Share Your Mic** — Sing into your phone or laptop, everyone hears you instantly
- **🔊 Share System Audio** — Playing a backing track? Share it with the room
- **🎵 Synchronized Music** — Stream music files that play in perfect sync across all devices
- **👥 Everyone Joins** — No setup, no accounts — just connect to the same network

## 🎯 Why This Exists

Commercial KTV systems cost thousands. Bluetooth speakers have annoying latency. Screen mirroring is clunky.

What if your existing devices could just... talk to each other? That's Wi-Fi Party — using UDP multicast, every device broadcasts and receives audio simultaneously, turning any local network into a karaoke room with latency low enough to actually sing along.

## ⚡ Built for Low Latency

Real-time audio is hard. We obsessed over every millisecond:

- **Lock-free queues** — No mutex contention in the audio path
- **3ms audio buffers** — As small as cpal allows
- **Adaptive jitter buffering** — Smooth playback without adding delay
- **Zero-copy serialization** — rkyv for network packets
- **DSCP/QoS marking** — Network priority for audio traffic

## 🚀 Quick Start

Build from source (see [HACKING.md](HACKING.md)).

Launch the app on each device. They'll automatically discover each other on the local network.

Tested on:

- macOS
- Android
- iOS

Windows and Linux **should** work with full function like macOS.
