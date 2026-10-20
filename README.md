# Genos Style Studio

A Windows desktop editor for Yamaha Genos Style files, built with Tauri 2, React and a Rust MIDI/CASM engine.

Edit source notes, shape controller data, assign voices and inspect routing while preserving the Style's timing and vendor chunks.

![Genos Style Studio with the transport and section bar, Style parts, piano roll, velocity lane and inspector](assets/piano-editor.png)

## Editing workspace

- **Transport and section bar:** play, stop and record with a tempo field, a compact time-signature control and a time readout, beside the fixed Genos section layout (Intro 1-3, Main A-D, Fill A-D, Break, Ending 1-3). Sections the loaded Style has are lit blue; missing ones stay clickable and open **Add section**.
- **Style parts:** the eight fixed parts as roomy cards with a channel number, an audible LED, a note-density strip and colour-coded Mute, Solo and Record switches.
- **Piano roll:** draw, move, resize, erase, select and copy notes, with snap, zoom and velocity editing.
- **Musical note properties:** a movable utility for pitch, start, length and velocity.
- **Quantize and note tools:** a grid preview that shows what the current size, swing, strength and catch range do, plus timing variation and controller movement, with Undo/Redo.
- **Part mixer:** volume faders, pan and send knobs, tone/EQ controls, mute, solo and record arm. Touching a strip selects that part.
- **Genos voice browser:** 2,456 built-in compatible addresses, including 58 kits; bank/category filters and complete paging. No PDF import is required for stock voices.
- **Style authoring:** section/source navigation, CASM inspection, controller/event editing, project recovery, `.GSS` projects and Style export.
- **Desktop MIDI:** device selection, playback, input monitoring, capture and MIDI exchange.
- **Desktop shell:** a frameless window with its own minimise, maximise and close buttons, in-app confirmation dialogs, a hint bar that names the control under the pointer and shows errors in place.

Compatibility work targets Genos/SFF2. Full arranger/CASM playback equivalence and broader hardware qualification remain in development.

Release qualification remains incomplete as of October 20, 2026. Sections added to a Style from the section bar have not been checked on hardware. The local workspace passes 316 Rust tests and 253 of 254 frontend tests (one App test, the initial-effects opt-in check, currently fails and is under investigation), but the source-only Rust regression suite still requires the ignored research fixtures. Expansion recovery is verified only for the exercised metadata, MIDI setup and existing PPF resource extraction paths; complete encrypted installation-to-editable-PPF reconstruction and Yamaha Expansion Manager/Genos acceptance remain unverified. This build is not approved as a stable release.

## Mixer controls

![Eight source-part mixer strips with volume faders, pan knobs and effect sends](assets/mixer-controls.png)

## Note properties and Quantize

Selected notes show their properties in the inspector, and the editor tools stay in movable desktop windows. Click either screenshot to inspect it at full size.

<table>
  <tr>
    <td width="50%"><a href="assets/note-properties.png"><img src="assets/note-properties.png" alt="Inspector showing a selected note with pitch, velocity, start and duration" /></a></td>
    <td width="50%"><a href="assets/quantize-controls.png"><img src="assets/quantize-controls.png" alt="Quantize window with the grid preview, affect options and feel knobs" /></a></td>
  </tr>
</table>

## Built-in Genos voices

The catalog includes 1,711 panel entries, 480 XG/GM entries and 265 GM2 entries. Voice assignment writes the selected MIDI bank/program address as one undoable edit. Recovered expansion addresses use their own selection path.

![Genos voice picker with built-in catalog, search, bank group and category controls](assets/voice-browser.png)

Screenshots were captured from the Windows desktop application on October 20, 2026.

## Build and run

Requirements:

- Windows 10/11, x64.
- Node.js 22.12 or newer and npm.
- Stable Rust with the MSVC toolchain.
- Visual Studio C++ Build Tools and a Windows SDK.
- Microsoft Edge WebView2 Runtime.

From the repository root:

```powershell
cd src
npm ci
npm run desktop
```

Create a Windows release and installer:

```powershell
cd src
npm run package
```

The executable is written to `src/target/release/`; the NSIS installer is written under `src/target/release/bundle/nsis/`. Dependency notices are generated during the build.

## Checks

```powershell
cd src
npm run verify
cargo test --workspace --all-targets
```

Corpus tests and native verification scripts also use local reference fixtures under `docs/`. Those research files and device-specific reports are excluded from this repository; the full corpus suite requires that local test data.

## Source layout

```text
assets/                  README screenshots
src/frontend/            React desktop interface and editor controls
src/crates/genos-core/   Style, MIDI, CASM and catalog processing
src/src-tauri/           Desktop shell, native MIDI and IPC
src/scripts/             Build helpers and desktop verification
```
