# Hacking on Wi-Fi Party

## 🛠️ Tech Stack

| Component      | Technology                                                                     |
| -------------- | ------------------------------------------------------------------------------ |
| UI             | [Dioxus](https://dioxuslabs.com/) Desktop                                      |
| Audio          | [cpal](https://github.com/RustAudio/cpal) + [Opus](https://opus-codec.org/)    |
| Music Decoding | [Symphonia](https://github.com/pdeljanov/Symphonia) (MP3, FLAC, OGG, WAV, AAC) |
| Network        | UDP Multicast                                                                  |
| Serialization  | [rkyv](https://github.com/rkyv/rkyv)                                           |

## 🏗️ How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                         Party (Orchestrator)                     │
├─────────────────────────────────────────────────────────────────┤
│  Mic Pipeline          System Audio Pipeline    Music Pipeline   │
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
│                    Network Layer (UDP Multicast)                 │
│         IPv4: 239.255.43.2:7667  │  IPv6: ff02::7667:7667       │
├─────────────────────────────────────────────────────────────────┤
│  Receive Pipeline                                                │
│  ┌──────────────────────────────────────────────────────────────┐│
│  │ NetworkReceiver → JitterBuffer → OpusDecoder → Mixer → Out  ││
│  │                 → SyncedStream (NTP-scheduled playback) ↗    ││
│  └──────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Prerequisites

- nix with flake support
- a shell with direnv

## Development Setup

You need two terminals running:

**Terminal 1** — Dioxus dev server:

```bash
dx serve

# For android:
dx serve --platform android

# For iOS, we use the system apple sdk (instead of nix's):
set -e DEVELOPER_DIR SDKROOT
set -gx PATH /usr/bin /bin /usr/sbin /sbin $PATH
export IPHONEOS_DEPLOYMENT_TARGET=10.0 # Prevent build/link target version mismatch
dx serve --platform ios
```

**Terminal 2** — Tailwind CSS watcher:

```bash
cd assets
npm i
npx @tailwindcss/cli -i tailwind.css -o tailwind_output.css --watch
```

## Building for Release

```bash
dx build --release
```

## Building for Android

```bash
dx build --platform android
```

The app automatically acquires `WifiManager.MulticastLock` via JNI to enable multicast UDP reception.

## Project Structure

```
src/
├── audio/          # Audio processing (buffers, effects, codecs)
├── io/             # Hardware I/O (cpal) & network (UDP multicast)
├── party/          # Main orchestration & pipeline wiring
├── pipeline/       # Generic Source/Sink/Node pipeline framework
├── state/          # Application state management
└── ui/             # Dioxus desktop UI components
```

## Architecture Overview

See [AGENTS.md](AGENTS.md) for detailed module documentation and data flow.
