# Hacking on Wi-Fi Party

## Prerequisites

- nix with flake support
- a shell with direnv

## Development Setup

You need two terminals running:

**Terminal 1** — Dioxus dev server:

```bash
dx serve
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
