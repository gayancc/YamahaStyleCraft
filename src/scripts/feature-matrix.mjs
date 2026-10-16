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
    "Pending",
    "Blank composer requires verified scaffold authoring",
    "—",
    "—",
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
    "Pending",
    "Clone helper is verified; shared CASM group serialization needs resolution",
    "—",
    "—",
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
  F011: ["Pending", "Recent files/home project library", "—", "—"],
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
    "Partial",
    "Correct numeric meter display; tempo/meter authoring pending",
    "document.rs, frontend/App.tsx",
    "retained_controls_expected_counts_and_meter",
  ],
  F016: [
    "Partial",
    "Verified Ctb2 root/chord/middle NTR+NTT; masks/regions pending",
    "document.rs, edit.rs, frontend/App.tsx",
    "casm_edits golden deltas; native-e2e",
  ],
  F017: [
    "Partial",
    "Numeric bank/program editing; named catalog mappings unknown",
    "frontend/App.tsx",
    "voice_bank_edits_match_retained_semantic_deltas",
  ],
  F018: [
    "Blocked by evidence",
    "No lawful Data List PDF + extraction oracle",
    "—",
    "—",
  ],
  F019: [
    "Pending evidence",
    "Device-specific drum rules are static surface only",
    "—",
    "—",
  ],
  F020: [
    "Partial",
    "CC7/10/11/91/93 file controls; session playback mute/solo",
    "frontend/App.tsx",
    "native-gestures CC7 edit; discrete event regressions",
  ],
  F021: [
    "Pending evidence",
    "No verified model-specific SysEx effect captures",
    "—",
    "—",
  ],
  F022: [
    "Implemented enumeration only",
    "WinMM scan, explicit output, name/index reconnect check",
    "midi.rs, frontend/App.tsx",
    "native-e2e enumerated FCB1010 + GS synth",
  ],
  F023: ["Pending", "Input monitoring not enabled", "—", "—"],
  F024: [
    "Pending evidence",
    "CASM chord preview transformations not verified",
    "—",
    "—",
  ],
  F025: [
    "Partial",
    "Native source playback/scheduler; no measured hardware or audio",
    "midi.rs",
    "native_plan timing; software-midi-smoke GS output and cancellation",
  ],
  F026: ["Pending", "MIDI recording/capture markers", "—", "—"],
  F027: [
    "Pending evidence",
    "Capture build, profile interpretation/precedence",
    "—",
    "—",
  ],
  F028: ["Pending", "CSV/JSON/profile exports", "—", "—"],
  F029: [
    "Pending evidence",
    "DAW exchange canonicalization unresolved",
    "—",
    "—",
  ],
  F030: [
    "Blocked by evidence",
    "No verified SFF1 conversion/corpus; opaque data preserved",
    "document.rs",
    "Ctab preservation only; conversion disabled",
  ],
  F031: [
    "Partial",
    "File/export/MIDI/error dialogs; full Help and advanced Fit pending",
    "frontend/App.tsx",
    "App tests; native-e2e",
  ],
  F032: [
    "Partial",
    "Cancel+release worker, shutdown cleanup; physical stuck-note tests pending",
    "midi.rs, src-tauri/src/main.rs",
    "native plan tests; real route test pending",
  ],
  F033: [
    "Partial",
    "Dirty/close guard, checksum, atomic autosave/restore; preferences pending",
    "project.rs, frontend/App.tsx",
    "project_self_contained; recovery_replacement; native recovery restore/clear",
  ],
  F034: [
    "Excluded",
    "Independent app does not reproduce proprietary licensing",
    "DEV-06-DECISIONS-AND-RISKS.md",
    "No licensing/runtime assets included",
  ],
  F035: [
    "Blocked by hardware",
    "No genuine Yamaha keyboard established",
    "—",
    "—",
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
fs.writeFileSync("../docs/development/DEV-03-FEATURE-IMPLEMENTATION-MATRIX.md", text);
console.log(
  `FEATURE MAPPING: ${rows.length} research surfaces, no omitted rows`,
);
