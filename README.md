# GenosStyleStudio

Independent Yamaha Style editor for Windows — open SFF2 Styles, edit notes on a piano roll, and keep original timing and vendor chunks intact. Built with Tauri 2, React, and a Rust engine.

<p align="center">
  <img src="docs/assets/piano-editor.png" alt="GenosStyleStudio piano roll editor showing Chord 1 notes in Main A" width="100%" />
</p>

<p align="center">
  <em>Piano roll · style sections · source channels · inspector — editing a loaded Style</em>
</p>

## What you get

- **Open Styles / projects** — `.STY`, `.PRS`, `.SST`, `.PST`, `.FPS`, MIDI, and `.GSS`
- **Piano roll editing** — select, draw, snap, quantize, velocity lane
- **Source channels** — Rhythm, Bass, Chord, Pad, Phrase with CASM-aware routing
- **Native MIDI** — playback, test-note audition, panic / all-notes-off
- **Voice & effects tools** — Genos Data List voices, DrumKit picker, system-effect SysEx apply
- **Preserving engine** — source timing and unknown Style chunks retained across edit/save

## Requirements

- Windows 10/11 (x64)
- [Rust](https://rustup.rs/) (stable)
- Node.js 20+
- WebView2 (usually already installed on Windows 11)

## Quick start

```bash
cd src
npm install
npm run desktop
```

That launches the Tauri app with the Vite frontend (`npm run desktop` → `tauri dev`).

### Other useful commands

```bash
cd src
npm run verify          # frontend build + vitest
cargo test -p genos-core --all-targets
cargo clippy --workspace --all-targets -- -D warnings
npm run package         # release installer (NSIS)
```

## Project layout

```
GenosStyleStudio/
├── docs/                 # development gates, research evidence, assets
│   └── assets/           # README screenshots
└── src/
    ├── frontend/         # React UI (piano roll, tools, dialogs)
    ├── crates/genos-core # Style/MIDI/CASM engine
    └── src-tauri/        # Tauri shell + IPC
```

## Screenshot fixture

The piano-roll image above was captured from a real session with `docs/assets/demo-style.STY` open (Main A · Chord 1).

## License & notices

Application source is developed independently. Third-party notices are generated under `docs/development/third-party/` when you build. Original SFM licensing and proprietary Yamaha assets are not redistributed.
