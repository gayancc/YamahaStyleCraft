import fs from "node:fs";
const original = fs.readFileSync(
  "../docs/research/findings/13-feature-coverage-matrix.md",
  "utf8",
);
const rows = original.split(/\r?\n/).filter((l) => /^\| F\d{3}/.test(l));
const statuses = {
  F001: [
    "Implemented",
    "Yamaha-only workspace; no platform chooser",
    "frontend/App.tsx",
    "native-e2e / App tests",
  ],
  F002: [
    "Partial · reference-based Composer",
    "Composer blank-from-reference; notes cleared; CASM/scaffold retained; synthetic no-scaffold blank and SFF1 authoring still blocked",
    "blank.rs, frontend/App.tsx Composer",
    "blank.rs unit tests; App Composer IPC test",
  ],
  F003: [
    "Implemented at tested scope",
    "Draw + numeric note editing; source precision",
    "frontend/PianoRoll.tsx, crates/genos-core/src/edit.rs",
    "note_edit_reopen; App velocity; native-e2e and expanded native gestures",
  ],
  F004: [
    "Implemented",
    "Native snapshot undo; 32 operations",
    "crates/genos-core/src/edit.rs",
    "note_edit_reopen; native-e2e",
  ],
  F005: [
    "Implemented",
    "Native redo, new edits invalidate branch",
    "crates/genos-core/src/edit.rs",
    "native-e2e; undo_branch_invalidates_redo",
  ],
  F006: [
    "Implemented",
    "Controller/program/bend authoring, native validation",
    "frontend/App.tsx EventInspector; edit.rs",
    "discrete_program_controller_and_bend; native event-add interaction",
  ],
  F007: [
    "Implemented at tested scope",
    "Section-local notes + CC/program/bend and voice baseline; inactive mask; used channels and boundary crossings reject",
    "casm.rs, frontend/SourceTracks.tsx",
    "source_tracks regressions; source-tracks-native.mjs",
  ],
  F008: [
    "Implemented at corpus scope",
    "84 Styles parse, exact no-op, numeric meter, preserve timing",
    "smf.rs, document.rs",
    "corpus_noop; retained_controls",
  ],
  F009: [
    "Implemented at tested scope",
    "Same-format edit/export with scaffold retained; no conversion",
    "edit.rs, project.rs",
    "native-e2e; verify-native-output.mjs",
  ],
  F010: [
    "Implemented with deliberate format change",
    "Self-contained .gss project; .sfmstyle interoperability pending",
    "project.rs",
    "project_self_contained; recovery_replacement; native recovery restore/clear; native-e2e",
  ],
  F011: [
    "Implemented at tested scope",
    "App-data recent-files.json (genos.recent_files.v1); home strip; open via open_document; missing→unavailable+remove; no MIDI on open",
    "recent.rs, src-tauri main.rs, frontend/App.tsx",
    "recent.rs unit/integration; App recent home IPC test",
  ],
  F012: [
    "Implemented",
    "Section/source-channel navigation",
    "frontend/App.tsx",
    "native-e2e",
  ],
  F013: [
    "Implemented at tested scope",
    "Draw/move/resize/delete/multiselect/copy-paste",
    "frontend/PianoRoll.tsx",
    "Native move/resize/draw/delete, Shift/marquee selection, clipboard, velocity, quantize and 1000px viewport tests",
  ],
  F014: [
    "Implemented; precision preserved",
    "Authored snap and explicit quantize; zoom/scroll",
    "frontend/PianoRoll.tsx, edit.rs",
    "quantize_clamps_exclusive_end; native quantize; render benchmark",
  ],
  F015: [
    "Partial · header tempo/meter",
    "Numeric meter display + tempo/meter SMF meta authoring with undo; section layout rewriting not in scope",
    "blank.rs apply_tempo_meter, edit.rs SetTempoMeter, frontend/App.tsx",
    "set_tempo_meter_updates_view_and_undoes; App tempo edit test",
  ],
  F016: [
    "Partial",
    "47-byte Ctb2 source/three-region rules, chord masks/exclusive assignment; shared groups isolated by section; keyboard chord transformation remains unqualified",
    "casm.rs, document.rs, frontend/SourceTracks.tsx",
    "golden deltas; source_tracks synthetic + corpus checks; actual native dialog/IPC",
  ],
  F017: [
    "Implemented at supported catalog scope",
    "Numeric editing plus local named search/resolve/confirm; one atomic bank/program operation at section start; normal MIDI inheritance remains visible",
    "voice_catalog.rs, frontend/VoiceBrowser.tsx, App voice inspector",
    "voice_catalog regressions, independent PDF oracle, VoiceBrowser tests and native workflow",
  ],
  F018: [
    "Implemented for verified Genos v2.00 PDF",
    "Fingerprint-bound 11/2019 English Data List import; all 1711 voice rows/cell addresses match independent extraction; other editions/models rejected; catalog remains local",
    "voice_catalog.rs, native catalog IPC, VoiceBrowser.tsx",
    "official PDF vs pdfplumber all-row comparison; persistence/integrity/errors; native import/search/apply/reopen",
  ],
  F019: [
    "Partial · Genos initial-header scalars",
    "49 imported kits; bounded Type-0 SFF2 per-key volume/pan/whole-semitone edits with stable unique ownership; sound remaps, advanced fields, other models, preview setup and hardware acceptance remain open",
    "native_drums.rs, voice_catalog.rs, frontend/NativeDrumSetup.tsx",
    "37 parameters across 84 Styles match independent parser; 3 eligible channel edits; golden bytes/undo/reopen plus native UI/IPC",
  ],
  F020: [
    "Implemented at tested scope",
    "CC7/10/11/91/93 file controls at section start; session mute/solo; model SysEx EQ remains F021",
    "frontend/App.tsx mixer",
    "native-gestures CC7 edit; discrete event regressions",
  ],
  F021: [
    "Partial · Genos system effects",
    "528 local types/descriptions; independent PDF import; bounded initial-header type/Return file edits and opt-in once-before-Play startup; no variation/insertion routing, detailed parameters or later effects replay",
    "effect_catalog.rs, genos_system_effect.rs, style_effects.rs, frontend/EffectBrowser.tsx",
    "catalog/corpus/golden/undo/reopen and native file/live separation; user confirmed direct and saved-file startup dry/wet reverb on Genos; keyboard Style loading and other effect algorithms open",
  ],
  F022: [
    "Implemented enumeration only",
    "WinMM scan, explicit output, name/index reconnect check",
    "midi.rs, frontend/App.tsx",
    "native-e2e enumerated FCB1010 + GS synth",
  ],
  F023: [
    "Implemented at tested scope",
    "Display-only exact-name input monitor; no recording/clock/Style capture build",
    "midi.rs InputMonitor, frontend/App.tsx device modal",
    "summarize unit tests; App start_monitor IPC test",
  ],
  F024: [
    "Partial · manual conversion study",
    "Explicit source/target chords; approximate pitched/guitar/expansion display; CASM binding, actual chord playback and full expansion qualification remain open",
    "chord_preview.rs, guitar_preview.rs, frontend/ChordPreview.tsx",
    "5632 Python pitched-contract cases, UI/native display checks; helper-only pitch-shift math",
  ],
  F025: [
    "Implemented at tested scope · hardware pilot",
    "Native playback + test-note audition + Genos section pilot; CASM chord / full Style acceptance still open (G7)",
    "midi.rs, examples/midi_probe.rs, frontend transport",
    "native_plan; software-midi-smoke; baseline-verify.json; App audition",
  ],
  F026: [
    "Implemented at tested scope · no Style build",
    "Start/stop/markers, Type-0 MIDI export, apply paired notes into current section; F027 Style/analyze-build remains gated",
    "capture.rs, frontend/App.tsx",
    "capture unit tests; App start_capture; apply_capture_to_section",
  ],
  F027: [
    "Pending evidence · UI gated",
    "Capture→Style/analyze-build not implemented; raw capture+apply notes only (F026)",
    "frontend feature inventory / capture panel",
    "—",
  ],
  F028: [
    "Partial · CSV + JSON snapshot",
    "Section CSV + genos.section_snapshot.v1 JSON noclobber; SFM profile interoperability remains open",
    "export_csv.rs, export_json.rs, frontend/App.tsx",
    "export_csv/json unit tests; App export IPC",
  ],
  F029: [
    "Partial · notes-only MIDI exchange",
    "Single-source phrase SMF export/preview/atomic note replacement; PPQ edge scaling and no-op comparison; DAW launch/session management and SFM equivalence remain open",
    "exchange.rs, frontend/MidiExchange.tsx, native phrase IPC",
    "exchange integration tests; MidiExchange UI tests; native export/preview/apply/undo/redo",
  ],
  F030: [
    "Blocked by evidence · UI gated",
    "No verified SFF1 conversion/corpus; opaque Ctab preserved; Composer documents gate",
    "document.rs, Composer copy",
    "Ctab preservation only; conversion disabled",
  ],
  F031: [
    "Implemented at tested scope · Fit deferred",
    "File/export/MIDI/error + Help + feature inventory/limits panel; advanced SFF2 Fit remains evidence-gated",
    "frontend/App.tsx",
    "App Help/inventory tests; native-e2e",
  ],
  F032: [
    "Partial · application release path",
    "Cancel/release and panic IPC; old unpaired release-check was invalidated by audit; corrected paired fixture is tested; physical silence qualification remains open",
    "midi.rs panic_release, examples/midi_probe panic-check, frontend panic",
    "midi_probe fixture positive/negative tests; historical pilot reports are not full stuck-note proof",
  ],
  F033: [
    "Implemented at tested scope",
    "Dirty/close guard, checksum, atomic autosave/restore; MIDI output + snap/zoom/tool prefs soft-fail on corrupt",
    "project.rs, prefs.rs, frontend/App.tsx",
    "project_self_contained; recovery_replacement; prefs corrupt soft-fail",
  ],
  F034: [
    "Excluded",
    "Independent app does not reproduce proprietary licensing",
    "DEV-06-DECISIONS-AND-RISKS.md",
    "No licensing/runtime assets included",
  ],
  F035: [
    "Partial · Genos baseline + G7 pilot",
    "Fresh 78 complete input pairs; user confirmed notes/Stop, source passage and direct/saved-file startup reverb contrast; prior reports retained; arranger Style loading/CASM/drums/clock/latency open",
    "midi.rs, scripts/genos-verify.mjs, genos-g7-pilot.mjs",
    "reports/genos-hardware/current-verified.json (raw reports + user observations); historical pilots separate",
  ],
};
let text =
  "# Feature implementation matrix\n\nUpdated 2026-10-16. Complete mapping of all 35 KNOWN investigation surfaces; the original inventory explicitly remains non-exhaustive. No aggregate completion percentage. Implemented does not imply every phase/release gate is complete.\n\n| ID / feature | Actual state | Scope / gap | Component | Verification |\n|---|---|---|---|---|\n";
for (const row of rows) {
  const [feature] = row
    .split("|")
    .slice(1)
    .map((s) => s.trim());
  const id = feature.slice(0, 4);
  if (!statuses[id]) throw Error(`Unmapped ${id}`);
  text += `| ${feature} | ${statuses[id].join(" | ")} |\n`;
}
fs.writeFileSync(
  "../docs/development/DEV-03-FEATURE-IMPLEMENTATION-MATRIX.md",
  text,
);
console.log(
  `FEATURE MAPPING: ${rows.length} research surfaces, no omitted rows`,
);
