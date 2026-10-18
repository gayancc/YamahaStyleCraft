// Owns its Vite/app children, synthetic fixture and fresh report directory.
// Does not open MIDI ports or transmit. Existing recovery bytes are restored.
import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { chromium, expect } from "@playwright/test";

assert.equal(
  process.platform,
  "win32",
  "Native Windows acceptance requires Windows",
);
const existing = execFileSync(
  "powershell",
  [
    "-NoProfile",
    "-Command",
    "Get-Process -Name genos-style-studio -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id; exit 0",
  ],
  { encoding: "utf8" },
);
assert.equal(
  existing.trim(),
  "",
  "Preserve any running user app; close it before this isolated check",
);
async function available(port) {
  try {
    await fetch(`http://127.0.0.1:${port}`);
    return false;
  } catch {
    return true;
  }
}
assert(await available(1420), "Vite port already in use");
assert(await available(9449), "CDP port already in use");
execFileSync("cargo", ["build", "--locked", "-p", "genos-style-studio"], {
  stdio: "inherit",
});
const run = path.resolve(
  `../docs/development/reports/source-tracks-${Date.now()}`,
);
fs.mkdirSync(run, { recursive: true });
const log = fs.openSync(path.join(run, "process.log"), "wx");
const pack = (tag, bytes) => {
  const n = Buffer.alloc(4);
  n.writeUInt32BE(bytes.length);
  return Buffer.concat([Buffer.from(tag), n, Buffer.from(bytes)]);
};
const route = (source) => {
  const b = Buffer.alloc(47);
  b[0] = source - 1;
  b[9] = 8;
  b[13] = 252;
  b[17] = 129;
  b[21] = 127;
  b[29] = 138;
  b[46] = 85;
  for (const o of [22, 28, 34]) b[o + 4] = 127;
  return b;
};
const vlq = (n) => {
  const bytes = [n & 127];
  while ((n >>= 7)) bytes.unshift((n & 127) | 128);
  return bytes;
};
const events = [
  [0, [0xff, 6, 4, ...Buffer.from("SFF2")]],
  [0, [0xb0, 0, 121]],
  [0, [0xb0, 32, 5]],
  [0, [0xc0, 34]],
  [10, [0xff, 6, 6, ...Buffer.from("Main A")]],
  [10, [0xb0, 7, 90]],
  [10, [0xe0, 0, 64]],
  [20, [0x90, 60, 100]],
  [30, [0x91, 62, 80]],
  [100, [0x80, 60, 0]],
  [120, [0x81, 62, 0]],
  [480, [0xff, 6, 6, ...Buffer.from("Main B")]],
  [500, [0x90, 64, 90]],
  [600, [0x80, 64, 0]],
  [960, [0xff, 0x2f, 0]],
];
let previous = 0;
const track = Buffer.from(
  events.flatMap(([tick, data]) => {
    const bytes = [...vlq(tick - previous), ...data];
    previous = tick;
    return bytes;
  }),
);
const records = Buffer.concat([
  pack("Sdec", Buffer.from("Main A,Main B")),
  pack("Ctb2", route(1)),
  pack("Xtra", [222, 173, 190, 239]),
  pack("Ctb2", route(2)),
]);
const source = path.join(run, "synthetic.STY");
const original = Buffer.concat([
  pack("MThd", [0, 0, 0, 1, 1, 224]),
  pack("MTrk", track),
  pack("CASM", Buffer.concat([pack("CSEG", records), pack("Tail", [4, 5, 6])])),
  pack("OTSc", [99, 88]),
]);
fs.writeFileSync(source, original, { flag: "wx" });
let vite,
  app,
  browser,
  recovery,
  recoveryBytes,
  recoveryCaptured = false;
const persistedFiles = new Map();
const ready = async (url, child) => {
  for (let i = 0; i < 150; i++) {
    assert.equal(
      child.exitCode,
      null,
      `Child exited; inspect ${run}/process.log`,
    );
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw Error(`Timed out waiting for ${url}`);
};
try {
  vite = spawn(
    process.execPath,
    [
      "node_modules/vite/bin/vite.js",
      "--host",
      "127.0.0.1",
      "--port",
      "1420",
      "--strictPort",
    ],
    { windowsHide: true, stdio: ["ignore", log, log] },
  );
  await ready("http://127.0.0.1:1420", vite);
  app = spawn(path.resolve("target/debug/genos-style-studio.exe"), [], {
    windowsHide: true,
    stdio: ["ignore", log, log],
    env: {
      ...process.env,
      WEBVIEW2_USER_DATA_FOLDER: path.join(run, "webview-profile"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9449",
    },
  });
  await ready("http://127.0.0.1:9449/json/version", app);
  browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
  const page = browser.contexts()[0].pages()[0];
  await page.waitForSelector(".brand");
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  recovery = await page.evaluate(async () => {
    const { appDataDir, join } =
      await import("/node_modules/@tauri-apps/api/path.js");
    return join(await appDataDir(), "recovery.gss");
  });
  recoveryBytes = fs.existsSync(recovery) ? fs.readFileSync(recovery) : null;
  for (const name of [
    "recovery.gss",
    "recent-files.json",
    "prefs.json",
    "voice-catalog.json",
    "effect-catalog.json",
  ]) {
    const file = path.join(path.dirname(recovery), name);
    persistedFiles.set(
      file,
      fs.existsSync(file) ? fs.readFileSync(file) : null,
    );
  }
  recoveryCaptured = true;
  if (recoveryBytes)
    await page
      .getByRole("button", { name: "Discard autosave", exact: true })
      .click();
  const ipc = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  const load = async (file) => {
    await page.evaluate(async (file) => {
      const { useStudio } = await import("/frontend/store.ts");
      await useStudio.getState().load(file);
      useStudio.getState().setChannel(1);
    }, file);
  };
  const state = () =>
    page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      return useStudio.getState().view;
    });
  await load(source);
  const before = await state();
  const section = before.sections.find((s) => s.label === "Main A");
  const bound = (v, label, source) =>
    v.routes.find((r) => r.source === source && r.sections.includes(label));
  await page.getByRole("button", { name: "CASM", exact: true }).click();
  await page
    .getByRole("button", { name: "Edit source tracks", exact: true })
    .click();
  const dialog = page.getByRole("dialog", { name: "Source tracks" });
  await dialog.getByLabel("Source root", { exact: true }).selectOption("7");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  assert.equal((await state()).revision, 0);
  await page
    .getByRole("button", { name: "Edit source tracks", exact: true })
    .click();
  await dialog
    .getByRole("button", { name: "Use part defaults", exact: true })
    .click();
  await expect(dialog.getByLabel("Region NTR", { exact: true })).toHaveValue(
    "1",
  );
  await expect(dialog.getByLabel("Region NTT", { exact: true })).toHaveValue(
    "0",
  );
  await expect(
    dialog.getByLabel("CASM Auto Start", { exact: true }),
  ).toBeChecked();
  await page.screenshot({ path: path.join(run, "part-defaults-draft.png") });
  await dialog
    .getByRole("button", { name: "Apply source changes", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  const defaultsView = await state();
  const defaults = bound(defaultsView, "Main A", 1).bytes;
  assert.equal(defaultsView.revision, 1);
  assert.deepEqual(
    defaults.slice(11, 22),
    [15, 255, 255, 255, 255, 255, 255, 0, 2, 0, 127],
  );
  for (const offset of [22, 28, 34])
    assert.deepEqual(defaults.slice(offset, offset + 6), [1, 0, 6, 0, 127, 1]);
  assert.equal(defaults[46], 85);
  assert.deepEqual(defaultsView.notes, before.notes);
  assert.deepEqual(
    bound(defaultsView, "Main A", 2).bytes,
    bound(before, "Main A", 2).bytes,
  );
  assert.deepEqual(
    bound(defaultsView, "Main B", 1).bytes,
    bound(before, "Main B", 1).bytes,
  );
  const defaultsPath = path.join(run, "part-defaults.STY");
  await ipc("save_document", { path: defaultsPath, projectFile: false });
  await load(defaultsPath);
  assert.deepEqual(bound(await state(), "Main A", 1).bytes, defaults);
  await load(source);
  await ipc("edit_document", {
    revision: 0,
    command: {
      type: "sourceTracks",
      section: before.sections.find((s) => s.label === "Main A").id,
      actions: [
        {
          type: "edit",
          source: 1,
          destination: 9,
          changes: [{ type: "partDefaults" }],
          chords: null,
          exclusive: false,
        },
      ],
    },
  });
  await ipc("history", { redo: false });
  const defaultsUndoPath = path.join(run, "part-defaults-undone.STY");
  await ipc("save_document", { path: defaultsUndoPath, projectFile: false });
  assert(fs.readFileSync(defaultsUndoPath).equals(original));
  fs.writeFileSync(
    path.join(run, "part-defaults-result.json"),
    JSON.stringify(
      {
        passed: true,
        destination: 9,
        bytes: defaults,
        singleRevision: true,
        notesAndOtherSourcesPreserved: true,
        otherSectionsPreserved: true,
        savedAndReopened: true,
        exactUndo: true,
        midiOpened: false,
      },
      null,
      2,
    ),
  );
  await load(source);
  await page
    .getByRole("button", { name: "Edit source tracks", exact: true })
    .click();
  await dialog.getByLabel("Source root", { exact: true }).selectOption("7");
  await dialog.getByLabel("Middle zone low").fill("36");
  await dialog.getByLabel("Allow chord root C", { exact: true }).check();
  await dialog.getByLabel("Allow chord root B", { exact: true }).check();
  await dialog.getByLabel("CASM Auto Start", { exact: true }).uncheck();
  await dialog.getByLabel("Middle zone high").fill("84");
  await dialog.getByRole("button", { name: "Low region", exact: true }).click();
  await dialog.getByLabel("Region NTR", { exact: true }).selectOption("2");
  await dialog.getByLabel("Region NTT", { exact: true }).selectOption("1");
  await dialog.getByLabel("NTT Bass", { exact: true }).check();
  await dialog.getByLabel("Output note low", { exact: true }).fill("10");
  await dialog.getByLabel("Output note high", { exact: true }).fill("100");
  await dialog
    .getByRole("button", { name: "High region", exact: true })
    .click();
  await dialog
    .getByLabel("Region retrigger", { exact: true })
    .selectOption("4");
  await dialog
    .getByRole("button", { name: "Major group", exact: true })
    .click();
  await dialog
    .getByRole("button", { name: "Minor group", exact: true })
    .click();
  await dialog
    .getByLabel("Assign enabled chords exclusively to this source")
    .check();
  await dialog.getByLabel("Clone into source channel").selectOption("3");
  await page.screenshot({ path: path.join(run, "source-track-draft.png") });
  await dialog
    .getByRole("button", { name: "Apply source changes", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  const edited = await state();
  assert.equal(edited.revision, 1);
  assert.equal(edited.notes.length, before.notes.length + 1);
  const a = bound(edited, "Main A", 1),
    clone = bound(edited, "Main A", 3);
  assert.equal(a.bytes[18], 7);
  assert.equal(a.bytes[11], 8);
  assert.equal(a.bytes[12], 1);
  assert.equal(a.bytes[13] & 4, 0);
  assert.deepEqual(a.bytes.slice(20, 22), [36, 84]);
  assert.deepEqual(a.bytes.slice(22, 28), [2, 129, 0, 10, 100, 0]);
  assert.equal(a.bytes[39], 4);
  for (const channel of [1, 2])
    assert.deepEqual(
      bound(edited, "Main B", channel).bytes,
      bound(before, "Main B", channel).bytes,
    );
  for (let c = 0; c < 34; c++) {
    const i = 17 - Math.floor(c / 8),
      bit = 1 << (c % 8);
    assert.equal(clone.bytes[i] & bit, 0);
    if (a.bytes[i] & bit)
      assert.equal(bound(edited, "Main A", 2).bytes[i] & bit, 0);
  }
  assert.equal(clone.bytes[13] & 252, 248);
  assert.equal(clone.bytes[46], 85);
  assert.deepEqual(
    edited.notes.filter((n) => n.channel !== 3),
    before.notes,
  );
  const output = path.join(run, "edited.STY"),
    project = path.join(run, "edited.gss");
  await ipc("save_document", { path: output, projectFile: false });
  await ipc("save_document", { path: project, projectFile: true });
  await ipc("history", { redo: false });
  const undo = await ipc("save_document", {
    path: path.join(run, "undone.STY"),
    projectFile: false,
  });
  assert.equal(undo.notes.length, before.notes.length);
  assert(fs.readFileSync(path.join(run, "undone.STY")).equals(original));
  await ipc("history", { redo: true });
  await ipc("save_document", {
    path: path.join(run, "redone.STY"),
    projectFile: false,
  });
  assert(
    fs
      .readFileSync(output)
      .equals(fs.readFileSync(path.join(run, "redone.STY"))),
  );
  await load(output);
  assert.deepEqual(bound(await state(), "Main A", 3).bytes, clone.bytes);
  await load(project);
  assert.equal((await state()).notes.length, edited.notes.length);
  await page.screenshot({ path: path.join(run, "source-track-result.png") });
  // Native late-action rejection leaves every byte and revision unchanged.
  const reopened = await state();
  let rejected = false;
  try {
    await ipc("edit_document", {
      revision: reopened.revision,
      command: {
        type: "sourceTracks",
        section: reopened.sections.find((s) => s.label === "Main A").id,
        actions: [
          {
            type: "edit",
            source: 1,
            destination: 9,
            changes: [{ type: "root", value: 2 }],
            chords: null,
            exclusive: false,
          },
          { type: "clone", source: 1, destination: 9, target: 2 },
        ],
      },
    });
  } catch {
    rejected = true;
  }
  assert(rejected);
  await ipc("save_document", {
    path: path.join(run, "rejected.STY"),
    projectFile: false,
  });
  assert(
    fs
      .readFileSync(path.join(run, "rejected.STY"))
      .equals(fs.readFileSync(output)),
  );
  assert(fs.readFileSync(source).equals(original));
  // Exercise Cursor's added native workflows, without opening any MIDI port.
  const selectedSection = reopened.sections.find((s) => s.label === "Main A");
  const csv = path.join(run, "section.csv"),
    jsonFile = path.join(run, "section.json");
  await ipc("export_section_csv", { path: csv, section: selectedSection.id });
  await ipc("export_section_json", {
    path: jsonFile,
    section: selectedSection.id,
  });
  const snapshot = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
  assert.equal(snapshot.schema, "genos.section_snapshot.v1");
  assert.equal(snapshot.notes.length, 3);
  assert.equal(fs.readFileSync(csv, "utf8").trim().split(/\r?\n/).length, 4);
  const savedCsv = fs.readFileSync(csv);
  let collision = false;
  try {
    await ipc("export_section_csv", { path: csv, section: selectedSection.id });
  } catch {
    collision = true;
  }
  assert(collision);
  assert(fs.readFileSync(csv).equals(savedCsv));
  await ipc("save_prefs", {
    prefs: {
      schema: "genos.prefs.v1",
      midiOutputName: null,
      midiOutputIndex: null,
      snap: 8,
      zoom: 1.25,
      tool: "draw",
    },
  });
  const prefs = await ipc("load_prefs");
  assert.equal(prefs.snap, 8);
  assert.equal(prefs.zoom, 1.25);
  assert.equal(prefs.midiOutputName, null);
  const recent = await ipc("list_recent");
  const sourceEntry = recent.find((r) => r.entry.path === source);
  assert(sourceEntry?.available);
  await ipc("remove_recent", { id: sourceEntry.entry.id });
  assert(
    !(await ipc("list_recent")).some(
      (r) => r.entry.id === sourceEntry.entry.id,
    ),
  );
  // Reload the real UI and open the native recent-file item it persisted.
  await page.reload();
  await page.waitForSelector(".brand");
  const recentHome = page.getByRole("region", { name: "Recent files" });
  await recentHome
    .getByRole("button")
    .filter({ hasText: path.basename(project) })
    .first()
    .click();
  await expect
    .poll(async () => (await state())?.notes.length)
    .toBe(edited.notes.length);
  const blank = await ipc("create_blank_document", {
    referencePath: source,
    options: {
      name: "Audited blank",
      tempoBpm: 100,
      numerator: 3,
      denominator: 4,
    },
  });
  assert.equal(blank.notes.length, 0);
  assert.equal(blank.dirty, true);
  assert.equal(blank.numerator, 3);
  assert(Math.abs(blank.tempo - 100) < 0.01);
  assert.equal(await ipc("recovery_status"), true);
  const blankProject = path.join(run, "blank.gss"),
    blankStyle = path.join(run, "blank.STY");
  await ipc("save_document", { path: blankProject, projectFile: true });
  await ipc("save_document", { path: blankStyle, projectFile: false });
  const readChunks = (buffer) => {
    const chunks = [];
    for (let offset = 0; offset < buffer.length;) {
      assert(offset + 8 <= buffer.length);
      const length = buffer.readUInt32BE(offset + 4);
      assert(offset + 8 + length <= buffer.length);
      chunks.push({
        tag: buffer.toString("ascii", offset, offset + 4),
        data: buffer.subarray(offset + 8, offset + 8 + length),
      });
      offset += 8 + length;
    }
    return chunks;
  };
  const originalChunks = readChunks(original),
    blankChunks = readChunks(fs.readFileSync(blankStyle));
  for (const tag of ["CASM", "OTSc", "MThd"])
    assert(
      blankChunks
        .find((c) => c.tag === tag)
        .data.equals(originalChunks.find((c) => c.tag === tag).data),
    );
  await load(blankProject);
  assert.equal((await state()).notes.length, 0);
  const capture = await ipc("capture_snapshot");
  assert.equal(capture.recording, false);
  assert.equal(capture.droppedEvents, 0);
  for (const [command, args] of [
    [
      "start_capture",
      { name: "GenosStyleStudio deliberately missing test input" },
    ],
    ["capture_marker", { label: "not recording" }],
    [
      "test_note",
      {
        index: 0,
        name: "missing",
        channel: 0,
        pitch: 60,
        velocity: 1,
        holdMs: 20,
      },
    ],
  ]) {
    let failed = false;
    try {
      await ipc(command, args);
    } catch {
      failed = true;
    }
    assert(failed, `${command} should reject`);
  }
  assert.equal((await ipc("capture_snapshot")).eventCount, 0);
  // Notes-only MIDI exchange: real engine/IPC and UI, scripted file-picker path only.
  await load(source);
  const exchangeBefore = await state();
  const exchangeSection = exchangeBefore.sections.find(
    (s) => s.label === "Main A",
  );
  const exchangeArgs = {
    section: exchangeSection.id,
    channel: 1,
    revision: exchangeBefore.revision,
  };
  const exportedMidi = path.join(run, "phrase.mid"),
    returnedMidi = path.join(run, "returned.mid");
  await ipc("export_phrase_midi", { ...exchangeArgs, path: exportedMidi });
  const exportedBytes = fs.readFileSync(exportedMidi);
  await assert.rejects(() =>
    ipc("export_phrase_midi", { ...exchangeArgs, path: exportedMidi }),
  );
  assert(fs.readFileSync(exportedMidi).equals(exportedBytes));
  const unchanged = await ipc("preview_phrase_midi", {
    ...exchangeArgs,
    path: exportedMidi,
  });
  assert.equal(unchanged.unchanged, true);
  assert.equal(
    (await ipc("apply_phrase_midi", { token: unchanged.token })).revision,
    0,
  );
  const encode = (entries) => {
    let previous = 0;
    return Buffer.from(
      entries.flatMap(([tick, data]) => {
        const out = [...vlq(tick - previous), ...data];
        previous = tick;
        return out;
      }),
    );
  };
  fs.writeFileSync(
    returnedMidi,
    Buffer.concat([
      pack("MThd", [0, 0, 0, 1, 1, 224]),
      pack(
        "MTrk",
        encode([
          [10, [0x96, 67, 91]],
          [90, [0x86, 67, 0]],
          [470, [0xff, 0x2f, 0]],
        ]),
      ),
    ]),
    { flag: "wx" },
  );
  const superseded = await ipc("preview_phrase_midi", {
    ...exchangeArgs,
    path: returnedMidi,
  });
  const current = await ipc("preview_phrase_midi", {
    ...exchangeArgs,
    path: returnedMidi,
  });
  await assert.rejects(() =>
    ipc("apply_phrase_midi", { token: superseded.token }),
  );
  await load(source); // Same revision, different document epoch must invalidate preview.
  await assert.rejects(() =>
    ipc("apply_phrase_midi", { token: current.token }),
  );
  await assert.rejects(() =>
    ipc("preview_phrase_midi", { ...exchangeArgs, path: source }),
  );
  // Tauri's native IPC is immutable. Substitute only the Vite file-picker
  // module for this UI check; every engine command still uses real native IPC.
  await page.route(
    "**/node_modules/.vite/deps/@tauri-apps_plugin-dialog.js*",
    (route) =>
      route.fulfill({
        contentType: "application/javascript",
        body: "export const open=async()=>globalThis.__exchangeFile; export const save=async()=>null; export const confirm=async()=>false;",
      }),
  );
  await page.reload();
  await page.waitForSelector(".brand");
  await page.evaluate((file) => {
    globalThis.__exchangeFile = file;
  }, returnedMidi);
  await load(source);
  const exchangeDialog = page.getByRole("dialog", {
    name: "MIDI exchange",
    exact: true,
  });
  await page
    .getByRole("button", { name: "MIDI exchange", exact: true })
    .click();
  await exchangeDialog
    .getByRole("button", { name: "Choose edited MIDI…" })
    .click();
  await expect(exchangeDialog.getByLabel("MIDI import preview")).toBeVisible();
  await exchangeDialog.getByText("Inspect 1 of 1 imported notes").click();
  await expect(
    exchangeDialog.getByRole("cell", { name: "G4 (67)", exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: path.join(run, "midi-exchange-preview.png") });
  await page.setViewportSize({ width: 1000, height: 640 });
  const viewport = await exchangeDialog.boundingBox();
  await page.screenshot({ path: path.join(run, "midi-exchange-minimum.png") });
  assert(
    viewport.x >= 0 &&
      viewport.y >= 0 &&
      viewport.x + viewport.width <= 1000 &&
      viewport.y + viewport.height <= 640,
    `Dialog must fit minimum viewport: ${JSON.stringify(viewport)}`,
  );
  const layout = await page.evaluate(() => ({
    width: window.innerWidth,
    scroll: document.documentElement.scrollWidth,
    overflow: [...document.querySelectorAll("body *")]
      .map((e) => ({
        tag: e.tagName,
        class: e.className,
        right: e.getBoundingClientRect().right,
      }))
      .filter((e) => e.right > window.innerWidth + 1)
      .slice(0, 12),
  }));
  fs.writeFileSync(
    path.join(run, "minimum-layout.json"),
    JSON.stringify(layout, null, 2),
  );
  assert(
    layout.scroll <= layout.width,
    `Minimum-width layout must not overflow horizontally: ${JSON.stringify(layout)}`,
  );
  await expect(
    exchangeDialog.getByRole("button", { name: "Apply MIDI notes" }),
  ).toBeInViewport();
  await expect(
    exchangeDialog.getByRole("button", { name: "Cancel", exact: true }),
  ).toBeInViewport();
  await page.setViewportSize({ width: 1480, height: 940 });
  await exchangeDialog
    .getByRole("button", { name: "Cancel", exact: true })
    .click();
  assert.equal((await state()).revision, 0);
  await page
    .getByRole("button", { name: "MIDI exchange", exact: true })
    .click();
  await exchangeDialog
    .getByRole("button", { name: "Choose edited MIDI…" })
    .click();
  await expect(exchangeDialog.getByLabel("MIDI import preview")).toBeVisible();
  // Change fixture on disk after preview: Apply must use the reviewed in-memory notes.
  const reviewedBytes = fs.readFileSync(returnedMidi);
  fs.writeFileSync(returnedMidi, Buffer.from("file changed after preview"));
  await exchangeDialog
    .getByRole("button", { name: "Apply MIDI notes" })
    .click();
  await expect(exchangeDialog).toHaveCount(0);
  fs.writeFileSync(returnedMidi, reviewedBytes);
  const importedView = await state();
  assert.equal(importedView.revision, 1);
  const importedNote = importedView.notes.find(
    (n) => n.channel === 1 && n.start < 480,
  );
  assert.deepEqual(
    [
      importedNote.start,
      importedNote.duration,
      importedNote.pitch,
      importedNote.velocity,
    ],
    [20, 80, 67, 91],
  );
  assert.deepEqual(
    importedView.notes.filter((n) => n.channel !== 1 || n.start >= 480),
    exchangeBefore.notes.filter((n) => n.channel !== 1 || n.start >= 480),
  );
  assert.deepEqual(importedView.routes, exchangeBefore.routes);
  assert.deepEqual(
    importedView.events.filter(
      (e) => (e.data[0] & 240) !== 128 && (e.data[0] & 240) !== 144,
    ),
    exchangeBefore.events.filter(
      (e) => (e.data[0] & 240) !== 128 && (e.data[0] & 240) !== 144,
    ),
  );
  const exchangeStyle = path.join(run, "exchange.STY");
  await ipc("save_document", { path: exchangeStyle, projectFile: false });
  const exchangeBytes = fs.readFileSync(exchangeStyle);
  for (const tag of ["MThd", "CASM", "OTSc"])
    assert(
      readChunks(exchangeBytes)
        .find((c) => c.tag === tag)
        .data.equals(originalChunks.find((c) => c.tag === tag).data),
    );
  await page
    .getByRole("button", { name: "Undo (Ctrl+Z)", exact: true })
    .click();
  await expect.poll(async () => (await state()).revision).toBe(2);
  const exchangeUndo = path.join(run, "exchange-undone.STY");
  await ipc("save_document", { path: exchangeUndo, projectFile: false });
  assert(fs.readFileSync(exchangeUndo).equals(original));
  await page
    .getByRole("button", { name: "Redo (Ctrl+Shift+Z)", exact: true })
    .click();
  await expect.poll(async () => (await state()).revision).toBe(3);
  const exchangeRedo = path.join(run, "exchange-redone.STY");
  await ipc("save_document", { path: exchangeRedo, projectFile: false });
  assert(fs.readFileSync(exchangeRedo).equals(exchangeBytes));
  const stalePreview = await ipc("preview_phrase_midi", {
    ...exchangeArgs,
    revision: 3,
    path: returnedMidi,
  });
  await ipc("history", { redo: false });
  await assert.rejects(() =>
    ipc("apply_phrase_midi", { token: stalePreview.token }),
  );
  // Official Data List is a local, ignored reference; no catalog is bundled.
  await load(source);
  const catalogPath = path.join(path.dirname(recovery), "voice-catalog.json");
  if (fs.existsSync(catalogPath)) fs.unlinkSync(catalogPath);
  const pdfPath = path.resolve(
    "../docs/development/voice-catalog/genos_en_dl_f0.pdf",
  );
  const pdfBytes = fs.readFileSync(pdfPath);
  await page.evaluate((file) => {
    globalThis.__exchangeFile = file;
  }, pdfPath);
  await page
    .getByRole("button", { name: "Browse Genos voices", exact: true })
    .click();
  const voicesDialog = page.getByRole("dialog", {
    name: "Genos voices",
    exact: true,
  });
  await voicesDialog
    .getByRole("button", { name: "Import Data List PDF…" })
    .click();
  await expect(voicesDialog.getByLabel("Search voices")).toBeVisible({
    timeout: 30000,
  });
  const catalogStatus = await ipc("voice_catalog_status");
  assert.equal(catalogStatus.count, 1711);
  assert.equal(
    catalogStatus.sourceSha256,
    "6022804163e4548abfd26db1889307d78f913297a3cc78b6cf6905bb3060a9bd",
  );
  const preservedCatalog = fs.readFileSync(catalogPath);
  await assert.rejects(() => ipc("import_voice_catalog", { path: source }));
  assert(fs.readFileSync(catalogPath).equals(preservedCatalog));
  await voicesDialog.getByLabel("Search voices").fill("CFX ConcertGrand");
  const voiceRow = voicesDialog
    .getByRole("button")
    .filter({ hasText: "CFX ConcertGrand" });
  await voiceRow.click();
  await expect(
    voicesDialog.getByLabel("Voice selection preview"),
  ).toContainText("Program 1");
  await page.screenshot({ path: path.join(run, "voice-catalog-preview.png") });
  await page.setViewportSize({ width: 1000, height: 640 });
  await expect(
    voicesDialog.getByRole("button", { name: "Apply voice", exact: true }),
  ).toBeInViewport();
  await expect(
    voicesDialog.getByRole("button", { name: "Cancel", exact: true }),
  ).toBeInViewport();
  assert(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  );
  await page.screenshot({ path: path.join(run, "voice-catalog-minimum.png") });
  await page.setViewportSize({ width: 1480, height: 940 });
  await voicesDialog
    .getByRole("button", { name: "Cancel", exact: true })
    .click();
  assert.equal((await state()).revision, 0);
  // Reload proves cache-backed resolution/search, independent of the importing UI.
  await page.reload();
  await page.waitForSelector(".brand");
  if (await ipc("recovery_status")) {
    await page
      .getByRole("button", { name: "Discard autosave", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Discard autosave", exact: true }),
    ).toHaveCount(0);
  }
  await load(source);
  assert.equal((await ipc("voice_catalog_status")).count, 1711);
  assert.equal(
    (await ipc("search_voice_catalog", { query: "CFX ConcertGrand" })).total,
    1,
  );
  const beforeVoice = await state();
  await page
    .getByRole("button", { name: "Browse Genos voices", exact: true })
    .click();
  await voicesDialog.getByLabel("Search voices").fill("CFX ConcertGrand");
  await voiceRow.click();
  await voicesDialog
    .getByRole("button", { name: "Apply voice", exact: true })
    .click();
  await expect(voicesDialog).toHaveCount(0);
  const withVoice = await state();
  assert.equal(withVoice.revision, 1);
  assert.deepEqual(withVoice.notes, beforeVoice.notes);
  assert.deepEqual(withVoice.routes, beforeVoice.routes);
  const targetSection = withVoice.sections.find((s) => s.label === "Main A");
  const named = await ipc("resolve_catalog_voice", {
    section: targetSection.id,
    channel: 1,
    revision: 1,
  });
  assert.deepEqual([named.msb, named.lsb, named.program], [104, 21, 0]);
  await expect(
    page.getByRole("button", { name: "Browse Genos voices", exact: true }),
  ).toContainText("CFX ConcertGrand");
  const voicedStyle = path.join(run, "voiced.STY"),
    voicedProject = path.join(run, "voiced.gss");
  await ipc("save_document", { path: voicedStyle, projectFile: false });
  await ipc("save_document", { path: voicedProject, projectFile: true });
  const voicedBytes = fs.readFileSync(voicedStyle);
  for (const tag of ["MThd", "CASM", "OTSc"])
    assert(
      readChunks(voicedBytes)
        .find((c) => c.tag === tag)
        .data.equals(originalChunks.find((c) => c.tag === tag).data),
    );
  await page
    .getByRole("button", { name: "Undo (Ctrl+Z)", exact: true })
    .click();
  await expect.poll(async () => (await state()).revision).toBe(2);
  const voiceUndo = path.join(run, "voice-undone.STY");
  await ipc("save_document", { path: voiceUndo, projectFile: false });
  assert(fs.readFileSync(voiceUndo).equals(original));
  await page
    .getByRole("button", { name: "Redo (Ctrl+Shift+Z)", exact: true })
    .click();
  await expect.poll(async () => (await state()).revision).toBe(3);
  const voiceRedo = path.join(run, "voice-redone.STY");
  await ipc("save_document", { path: voiceRedo, projectFile: false });
  assert(fs.readFileSync(voiceRedo).equals(voicedBytes));
  await assert.rejects(() =>
    ipc("apply_catalog_voice", {
      section: targetSection.id,
      channel: 1,
      revision: 0,
      address: { msb: 104, lsb: 21, program: 0 },
    }),
  );
  await assert.rejects(() =>
    ipc("apply_catalog_voice", {
      section: targetSection.id,
      channel: 1,
      revision: 3,
      address: { msb: 127, lsb: 127, program: 127 },
    }),
  );
  await load(voicedProject);
  const reopenedVoice = await state();
  const musicalNotes = (notes) =>
    notes.map(({ start, duration, channel, pitch, velocity, track }) => ({
      start,
      duration,
      channel,
      pitch,
      velocity,
      track,
    }));
  assert.deepEqual(
    musicalNotes(reopenedVoice.notes),
    musicalNotes(beforeVoice.notes),
  );
  assert.equal(
    (
      await ipc("resolve_catalog_voice", {
        section: reopenedVoice.sections.find((s) => s.label === "Main A").id,
        channel: 1,
        revision: 0,
      })
    ).name,
    "CFX ConcertGrand",
  );
  assert(fs.readFileSync(pdfPath).equals(pdfBytes));
  assert(fs.readFileSync(source).equals(original));
  // Reviewed Cursor additions: actual extraction/IPC/UI, no output selected or opened.
  const effectPath = path.join(path.dirname(recovery), "effect-catalog.json");
  await page.evaluate((file) => {
    globalThis.__exchangeFile = file;
  }, pdfPath);
  if (fs.existsSync(effectPath)) fs.unlinkSync(effectPath);
  await page.getByRole("button", { name: "Effect types", exact: true }).click();
  const effectsDialog = page.getByRole("dialog", {
    name: "Genos effect types",
    exact: true,
  });
  await effectsDialog
    .getByRole("button", { name: "Import effect Data List PDF", exact: true })
    .click();
  await expect(effectsDialog.getByLabel("Search effect types")).toBeEnabled({
    timeout: 30000,
  });
  assert.equal((await ipc("effect_catalog_status")).count, 528);
  const effectCache = fs.readFileSync(effectPath),
    voiceCache = fs.readFileSync(catalogPath);
  await assert.rejects(() => ipc("import_effect_catalog", { path: source }));
  assert(fs.readFileSync(effectPath).equals(effectCache));
  assert(fs.readFileSync(catalogPath).equals(voiceCache));
  await effectsDialog
    .getByLabel("Search effect types")
    .fill("Real Large Hall +");
  await effectsDialog.getByRole("option", { name: /Real Large Hall/ }).click();
  await expect(effectsDialog.getByLabel("Selected effect type")).toContainText(
    "rich middle range",
  );
  await expect(
    effectsDialog.getByRole("button", { name: /Apply to Genos/ }),
  ).toBeDisabled();
  await effectsDialog.getByLabel("Effect block").selectOption("variation");
  await expect(effectsDialog.getByLabel("Selected effect type")).toHaveCount(0);
  await page.setViewportSize({ width: 1000, height: 640 });
  await expect(
    effectsDialog.getByRole("button", { name: "Close", exact: true }).first(),
  ).toBeInViewport();
  await expect(
    effectsDialog.getByRole("button", { name: /Apply to Genos/ }),
  ).toBeInViewport();
  await page.screenshot({
    path: path.join(run, "effects-reviewed-minimum.png"),
  });
  await effectsDialog
    .getByRole("button", { name: "Close", exact: true })
    .first()
    .click();
  await assert.rejects(() =>
    ipc("apply_genos_system_effect", {
      index: 999999,
      name: "Not a device",
      block: "reverb",
      msb: 127,
      lsb: 127,
    }),
  );
  await assert.rejects(() =>
    ipc("apply_genos_system_effect", {
      index: 999999,
      name: "Not a device",
      block: "reverb",
      msb: 1,
      returnLevel: 64,
    }),
  );
  await assert.rejects(() =>
    ipc("preview_drum_note", {
      index: 999999,
      name: "Not a device",
      channel: 9,
      pitch: 92,
    }),
  );
  const tie = await ipc("remap_chord_preview_notes", {
    notes: [60],
    targetRoot: 6,
    targetQuality: "Major",
    ntr: "root_fixed",
    ntt: "bypass",
  });
  assert.equal(tie[0].preview, 66);
  await assert.rejects(() =>
    ipc("remap_chord_preview_notes", {
      notes: [],
      targetRoot: 12,
      targetQuality: "Major",
    }),
  );
  const unchangedPreview = await state();
  await page
    .getByRole("button", { name: "Chord preview", exact: true })
    .click();
  const chordDialog = page.getByRole("dialog", {
    name: "Chord preview",
    exact: true,
  });
  await chordDialog.getByLabel("Source chord root").selectOption("2");
  await chordDialog.getByLabel("Source chord quality").selectOption("Minor");
  await chordDialog.getByLabel("Preview chord root").selectOption("6");
  await expect(
    chordDialog.getByRole("button", { name: "Close", exact: true }).last(),
  ).toBeInViewport();
  await page.screenshot({ path: path.join(run, "chord-reviewed-minimum.png") });
  await chordDialog
    .getByRole("button", { name: "Close", exact: true })
    .first()
    .click();
  assert.deepEqual(await state(), unchangedPreview);
  await page
    .locator(".track")
    .filter({ has: page.locator(".channel-number", { hasText: /^09$/ }) })
    .click();
  await page
    .getByRole("button", { name: "Drum kits (Ch 9–10)", exact: true })
    .click();
  const drumDialog = page.getByRole("dialog", {
    name: "Drum kits",
    exact: true,
  });
  await drumDialog.getByRole("option", { name: /RockDrumKit/ }).click();
  await expect(
    drumDialog.getByRole("button", { name: "Audition note", exact: true }),
  ).toBeDisabled();
  await expect(
    drumDialog.getByRole("button", { name: "Apply kit to Ch 9", exact: true }),
  ).toBeInViewport();
  await page.screenshot({ path: path.join(run, "drums-reviewed-minimum.png") });
  await drumDialog
    .getByRole("button", { name: "Apply kit to Ch 9", exact: true })
    .click();
  await expect(drumDialog).toHaveCount(0);
  const drumState = await state();
  const drumStatus = await ipc("drum_channel_status", {
    section: drumState.sections.find((s) => s.label === "Main A").id,
    revision: drumState.revision,
  });
  assert.equal(drumStatus[0].voice.name, "RockDrumKit");
  assert.deepEqual(drumState.notes, unchangedPreview.notes);
  assert.deepEqual(drumState.routes, unchangedPreview.routes);
  await page.setViewportSize({ width: 1480, height: 940 });
  assert.deepEqual(errors, []);
  // Independent synthetic initial-header fixture for Genos scalar editing.
  const nativeDrumFile = path.join(run, "native-drums.STY");
  const drumRows = [
    [0, [0xff, 6, 4, ...Buffer.from("SFF2")]],
    [1, [0xb8, 0, 127]],
    [1, [0xb8, 32, 0]],
    [1, [0xc8, 0]],
    [1, [0xb9, 0, 127]],
    [1, [0xb9, 32, 0]],
    [1, [0xc9, 0]],
    [2, [0xf0, 8, 0x43, 0x10, 0x4c, 8, 8, 7, 3, 0xf7]],
    [3, [0xf0, 8, 0x43, 0x10, 0x4c, 8, 9, 7, 2, 0xf7]],
    [4, [0xf0, 8, 0x43, 0x10, 0x4c, 0x31, 36, 2, 77, 0xf7]],
    [100, [0xff, 6, 6, ...Buffer.from("Main A")]],
    [110, [0x98, 36, 90]],
    [140, [0x88, 36, 0]],
    [200, [0xff, 6, 6, ...Buffer.from("Main B")]],
    [210, [0x99, 38, 80]],
    [240, [0x89, 38, 0]],
    [300, [0xff, 0x2f, 0]],
  ];
  let drumTick = 0;
  const drumTrack = Buffer.concat(
    drumRows.map(([tick, data]) => {
      const out = Buffer.from([...vlq(tick - drumTick), ...data]);
      drumTick = tick;
      return out;
    }),
  );
  const rhythm10 = route(10);
  rhythm10[9] = 9;
  const drumOriginal = Buffer.concat([
    pack("MThd", [0, 0, 0, 1, 1, 224]),
    pack("MTrk", drumTrack),
    pack(
      "CASM",
      pack(
        "CSEG",
        Buffer.concat([
          pack("Sdec", Buffer.from("Main A,Main B")),
          pack("Ctb2", route(9)),
          pack("Ctb2", rhythm10),
          pack("Xtra", [0xfa, 0xce]),
        ]),
      ),
    ),
    pack("OTSc", [1, 2, 3]),
  ]);
  fs.writeFileSync(nativeDrumFile, drumOriginal, { flag: "wx" });
  await load(nativeDrumFile);
  await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    useStudio.getState().setChannel(9);
  });
  const openNative = async () => {
    await page
      .getByRole("button", { name: "Drum kits (Ch 9–10)", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Genos per-key setup…", exact: true })
      .click();
    await expect(
      page
        .getByRole("dialog", { name: "Genos per-key Drum Setup", exact: true })
        .getByLabel("Native drum key"),
    ).toBeVisible();
  };
  await openNative();
  const nativeDialog = page.getByRole("dialog", {
    name: "Genos per-key Drum Setup",
    exact: true,
  });
  await nativeDialog.getByLabel("Drum Volume", { exact: true }).fill("101");
  await nativeDialog
    .getByRole("button", { name: "Cancel", exact: true })
    .click();
  assert.equal((await state()).revision, 0);
  await openNative();
  await nativeDialog.getByLabel("Drum Volume", { exact: true }).fill("101");
  await nativeDialog.getByLabel("Drum Pan", { exact: true }).fill("20");
  await nativeDialog
    .getByLabel("Drum Pitch (semitones)", { exact: true })
    .fill("-2");
  await page.setViewportSize({ width: 1000, height: 640 });
  await expect(
    nativeDialog.getByRole("button", { name: "Apply Drum Setup", exact: true }),
  ).toBeInViewport();
  await page.screenshot({ path: path.join(run, "native-drums-minimum.png") });
  await nativeDialog
    .getByRole("button", { name: "Apply Drum Setup", exact: true })
    .click();
  await expect(nativeDialog).toHaveCount(0);
  const nativeEdited = await state();
  assert.equal(nativeEdited.revision, 1);
  const header = await ipc("inspect_native_drums", { channel: 9, revision: 1 });
  const kick = header.header.keys.find((k) => k.note === 36);
  assert.deepEqual([kick.volume, kick.pan, kick.pitch], [101, 20, -2]);
  assert.deepEqual(
    nativeEdited.notes,
    (await ipc("open_document", { path: nativeDrumFile })).notes,
  );
  // Reopen through the store; do not leave its view detached from the native session.
  await load(nativeDrumFile);
  await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    useStudio.getState().setChannel(9);
  });
  const drumChanges = [
    { note: 36, field: "volume", value: 101 },
    { note: 36, field: "pan", value: 20 },
    { note: 36, field: "pitch", value: -2 },
  ];
  const drumArgs = {
    channel: 9,
    revision: 0,
    address: { msb: 127, lsb: 0, program: 0 },
    changes: drumChanges,
  };
  const drumApplied = await ipc("apply_native_drums", drumArgs);
  const nativeOutput = path.join(run, "native-drums-edited.STY"),
    nativeProject = path.join(run, "native-drums.gss");
  await ipc("save_document", { path: nativeOutput, projectFile: false });
  await ipc("save_document", { path: nativeProject, projectFile: true });
  const drumBytes = fs.readFileSync(nativeOutput);
  for (const tag of ["MThd", "CASM", "OTSc"])
    assert(
      readChunks(drumBytes)
        .find((c) => c.tag === tag)
        .data.equals(readChunks(drumOriginal).find((c) => c.tag === tag).data),
    );
  await assert.rejects(() => ipc("apply_native_drums", drumArgs));
  await assert.rejects(() =>
    ipc("edit_document", {
      revision: 1,
      command: {
        type: "setDrumScalars",
        channel: 9,
        address: drumArgs.address,
        changes: drumChanges,
      },
    }),
  );
  await assert.rejects(() =>
    ipc("apply_native_drums", {
      ...drumArgs,
      revision: 1,
      address: { msb: 127, lsb: 127, program: 127 },
    }),
  );
  await ipc("history", { redo: false });
  const nativeUndo = path.join(run, "native-drums-undo.STY");
  await ipc("save_document", { path: nativeUndo, projectFile: false });
  assert(fs.readFileSync(nativeUndo).equals(drumOriginal));
  await ipc("history", { redo: true });
  const nativeRedo = path.join(run, "native-drums-redo.STY");
  await ipc("save_document", { path: nativeRedo, projectFile: false });
  assert(fs.readFileSync(nativeRedo).equals(drumBytes));
  for (const file of [nativeOutput, nativeProject]) {
    await load(file);
    const current = await ipc("inspect_native_drums", {
      channel: 9,
      revision: 0,
    });
    const key = current.header.keys.find((k) => k.note === 36);
    assert.deepEqual([key.volume, key.pan, key.pitch], [101, 20, -2]);
  }
  await load(source);
  await assert.rejects(() =>
    ipc("inspect_native_drums", { channel: 9, revision: 0 }),
  );
  assert(fs.readFileSync(nativeDrumFile).equals(drumOriginal));
  // Effect1 fixture: a checksummed bulk block must survive typed overrides exactly.
  const effectFile = path.join(run, "header-effects.STY");
  const bulkData = [1, 0, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 64, 15];
  const bulkBody = [0, 14, 2, 1, 0, ...bulkData];
  const bulkPayload = [
    0x43,
    0,
    0x4c,
    ...bulkBody,
    -bulkBody.reduce((a, b) => a + b, 0) & 127,
    0xf7,
  ];
  const effectRows = [
    [0, [0xff, 6, 4, ...Buffer.from("SFF2")]],
    [1, [0xf0, 8, 0x43, 0x10, 0x4c, 0, 0, 0x7e, 0, 0xf7]],
    [2, [0xf0, ...vlq(bulkPayload.length), ...bulkPayload]],
    [3, [0xf0, 9, 0x43, 0x10, 0x4c, 2, 1, 32, 65, 0, 0xf7]],
    ...events.filter(([tick]) => tick >= 10),
  ];
  let effectTick = 0;
  const effectOriginal = Buffer.concat([
    pack("MThd", [0, 0, 0, 1, 1, 224]),
    pack(
      "MTrk",
      Buffer.from(
        effectRows.flatMap(([tick, data]) => {
          const out = [...vlq(tick - effectTick), ...data];
          effectTick = tick;
          return out;
        }),
      ),
    ),
    ...readChunks(original)
      .filter((c) => !["MThd", "MTrk"].includes(c.tag))
      .map((c) => pack(c.tag, c.data)),
  ]);
  fs.writeFileSync(effectFile, effectOriginal, { flag: "wx" });
  await load(effectFile);
  const effectBefore = await state();
  const openEffects = async () => {
    await page
      .getByRole("button", { name: "Effect types", exact: true })
      .click();
    await expect(
      effectsDialog.getByLabel("Initial Style effect"),
    ).toContainText("Return 64");
    await effectsDialog
      .getByLabel("Search effect types")
      .fill("Real Large Hall +");
    await effectsDialog
      .getByRole("option", { name: /Real Large Hall/ })
      .click();
    await effectsDialog.getByLabel("System effect return level").fill("100");
  };
  await openEffects();
  await effectsDialog
    .getByRole("button", { name: "Close", exact: true })
    .first()
    .click();
  assert.equal((await state()).revision, 0);
  await openEffects();
  await page.setViewportSize({ width: 1000, height: 640 });
  await expect(
    effectsDialog.getByRole("button", { name: "Apply to Style", exact: true }),
  ).toBeInViewport();
  await expect(
    effectsDialog.getByRole("button", { name: "Apply to Genos", exact: true }),
  ).toBeInViewport();
  await expect(
    effectsDialog.getByRole("button", { name: "Apply to Genos", exact: true }),
  ).toBeDisabled();
  await page.screenshot({ path: path.join(run, "style-effects-minimum.png") });
  await effectsDialog
    .getByRole("button", { name: "Apply to Style", exact: true })
    .click();
  await expect(effectsDialog).toHaveCount(0);
  const effectAfter = await state();
  assert.equal(effectAfter.revision, 1);
  assert.deepEqual(effectAfter.notes, effectBefore.notes);
  const appliedHeader = await ipc("inspect_style_effects", { revision: 1 });
  assert.deepEqual(appliedHeader.reverb, {
    msb: 1,
    lsb: 35,
    returnLevel: 100,
    lateChanges: false,
  });
  assert.deepEqual(appliedHeader.chorus, {
    msb: 65,
    lsb: 0,
    returnLevel: null,
    lateChanges: false,
  });
  const effectOutput = path.join(run, "header-effects-edited.STY"),
    effectProject = path.join(run, "header-effects.gss");
  await ipc("save_document", { path: effectOutput, projectFile: false });
  await ipc("save_document", { path: effectProject, projectFile: true });
  const effectBytes = fs.readFileSync(effectOutput);
  assert(
    effectBytes.includes(Buffer.from(bulkPayload)),
    "Original bulk count/data/checksum preserved",
  );
  for (const tag of ["MThd", "CASM", "OTSc"])
    assert(
      readChunks(effectBytes)
        .find((c) => c.tag === tag)
        .data.equals(
          readChunks(effectOriginal).find((c) => c.tag === tag).data,
        ),
    );
  const effectChanges = [
    { block: "reverb", effect: { msb: 1, lsb: 35 }, returnLevel: 100 },
  ];
  const effectUnchanged = await ipc("apply_style_effects", {
    revision: 1,
    changes: effectChanges,
  });
  assert.equal(effectUnchanged.revision, 1);
  await assert.rejects(() =>
    ipc("apply_style_effects", { revision: 0, changes: effectChanges }),
  );
  await assert.rejects(() =>
    ipc("apply_style_effects", {
      revision: 1,
      changes: [{ block: "reverb", effect: { msb: 127, lsb: 127 } }],
    }),
  );
  await assert.rejects(() =>
    ipc("edit_document", {
      revision: 1,
      command: { type: "setStyleEffects", changes: effectChanges },
    }),
  );
  await ipc("history", { redo: false });
  const effectUndo = path.join(run, "header-effects-undo.STY");
  await ipc("save_document", { path: effectUndo, projectFile: false });
  assert(fs.readFileSync(effectUndo).equals(effectOriginal));
  await ipc("history", { redo: true });
  const effectRedo = path.join(run, "header-effects-redo.STY");
  await ipc("save_document", { path: effectRedo, projectFile: false });
  assert(fs.readFileSync(effectRedo).equals(effectBytes));
  for (const file of [effectOutput, effectProject]) {
    await load(file);
    assert.deepEqual(
      (await ipc("inspect_style_effects", { revision: 0 })).reverb,
      appliedHeader.reverb,
    );
    assert.deepEqual(
      musicalNotes((await state()).notes),
      musicalNotes(effectBefore.notes),
    );
  }
  // Native cache boundary: Return alone is independent of PDF catalog availability.
  fs.unlinkSync(effectPath);
  try {
    assert.equal(await ipc("effect_catalog_status"), null);
    await assert.rejects(() =>
      ipc("apply_style_effects", { revision: 0, changes: effectChanges }),
    );
    const returnOnly = await ipc("apply_style_effects", {
      revision: 0,
      changes: [{ block: "chorus", returnLevel: 0 }],
    });
    assert.equal(returnOnly.revision, 1);
    assert.equal(
      (await ipc("inspect_style_effects", { revision: 1 })).chorus.returnLevel,
      0,
    );
  } finally {
    fs.writeFileSync(effectPath, effectCache);
  }
  assert(fs.readFileSync(effectFile).equals(effectOriginal));
  await load(effectOutput);
  const startup = await ipc("preview_style_effects", { revision: 0 });
  const startupCache = fs.readFileSync(effectPath);
  fs.unlinkSync(effectPath);
  try {
    assert.equal(await ipc("effect_catalog_status"), null);
    assert.deepEqual(
      (await ipc("preview_style_effects", { revision: 0 })).packets,
      startup.packets,
    );
  } finally {
    fs.writeFileSync(effectPath, startupCache);
  }
  assert.deepEqual(startup.packets, [
    [0xf0, 0x43, 0x10, 0x4c, 2, 1, 0, 1, 35, 0xf7],
    [0xf0, 0x43, 0x10, 0x4c, 2, 1, 12, 100, 0xf7],
    [0xf0, 0x43, 0x10, 0x4c, 2, 1, 32, 65, 0, 0xf7],
  ]);
  assert.equal((await state()).revision, 0);
  await assert.rejects(
    () => ipc("preview_style_effects", { revision: 1 }),
    /Document changed/,
  );
  // Read-only device scan and native preflight. No playback on an actual port.
  await page
    .getByRole("button", { name: "MIDI device settings", exact: true })
    .click();
  const midiDialog = page.getByRole("dialog", {
    name: "MIDI connections",
    exact: true,
  });
  await expect(
    midiDialog.getByLabel("Preview initial Genos effects"),
  ).not.toBeChecked();
  await midiDialog
    .getByLabel("MIDI output", { exact: true })
    .selectOption({ label: "Microsoft GS Wavetable Synth" });
  await midiDialog.getByLabel("Preview initial Genos effects").click();
  await expect(
    midiDialog.getByLabel("Preview initial Genos effects"),
  ).toBeChecked();
  await expect(midiDialog.getByRole("status")).toContainText(
    "3 typed messages ready",
  );
  await page.setViewportSize({ width: 1000, height: 640 });
  await midiDialog
    .getByLabel("Preview initial Genos effects")
    .scrollIntoViewIfNeeded();
  assert(
    await midiDialog.evaluate((el) => el.scrollWidth <= el.clientWidth),
    "MIDI dialog must not overflow horizontally",
  );
  assert(
    await midiDialog.evaluate((el) => {
      const frame = el.getBoundingClientRect();
      return [...el.querySelectorAll("button")].every((button) => {
        const rect = button.getBoundingClientRect();
        return rect.left >= frame.left && rect.right <= frame.right;
      });
    }),
    "MIDI action buttons must remain inside the dialog width",
  );
  await expect(
    midiDialog.getByLabel("Preview initial Genos effects"),
  ).toBeInViewport();
  await page.screenshot({
    path: path.join(run, "initial-effects-minimum.png"),
  });
  await midiDialog.getByLabel("MIDI output", { exact: true }).selectOption("");
  await midiDialog
    .getByLabel("MIDI output", { exact: true })
    .selectOption({ label: "Microsoft GS Wavetable Synth" });
  await expect(
    midiDialog.getByLabel("Preview initial Genos effects"),
  ).not.toBeChecked();
  await midiDialog.getByLabel("Preview initial Genos effects").click();
  await expect(
    midiDialog.getByLabel("Preview initial Genos effects"),
  ).toBeChecked();
  await expect(midiDialog.getByRole("status")).toContainText(
    "3 typed messages ready",
  );
  await load(effectOutput);
  await expect(
    midiDialog.getByLabel("Preview initial Genos effects"),
  ).not.toBeChecked();
  await midiDialog.getByLabel("MIDI output", { exact: true }).selectOption("");
  await midiDialog.getByRole("button", { name: "Done", exact: true }).click();
  const playArgs = {
    section: (await state()).sections[0].id,
    channels: [1],
    index: 999999,
    name: "Never opened",
    looping: false,
  };
  await assert.rejects(
    () => ipc("play", { ...playArgs, effectsToken: startup.token }),
    /Document changed/,
  );
  const replacement = await ipc("preview_style_effects", { revision: 0 });
  await ipc("apply_style_effects", {
    revision: 0,
    changes: [{ block: "reverb", returnLevel: 99 }],
  });
  await assert.rejects(
    () => ipc("play", { ...playArgs, effectsToken: replacement.token }),
    /Document changed/,
  );
  const currentStartup = await ipc("preview_style_effects", { revision: 1 });
  await assert.rejects(
    () =>
      ipc("play", {
        ...playArgs,
        channels: [0],
        effectsToken: currentStartup.token,
      }),
    /invalid enabled channel/,
  );
  await load(source);
  await assert.rejects(
    () => ipc("preview_style_effects", { revision: 0 }),
    /No explicit initial/,
  );
  assert.deepEqual(errors, []);
  const report = {
    result: "SOURCE TRACKS NATIVE VERIFIED",
    run,
    source,
    output,
    project,
    revision: edited.revision,
    notesBefore: before.notes.length,
    notesAfter: edited.notes.length,
    checks: [
      "UI Cancel",
      "UI combined Apply",
      "all region fields",
      "exclusive chord masks",
      "shared sibling sections unchanged",
      "inactive source clone",
      "undo byte-exact",
      "redo byte-exact",
      "Style and project reopen",
      "late batch failure atomic",
      "input preserved",
      "native Composer blank + autosave + Style/GSS reopen",
      "native recent list/remove and actual home item open",
      "native preferences roundtrip with output deselected",
      "native CSV/JSON exports + noclobber",
      "capture/audition invalid input boundaries",
      "MIDI phrase export + noclobber + unchanged return",
      "MIDI preview superseded/replaced-document/revision tokens reject",
      "actual MIDI exchange UI preview/Cancel/Apply with scripted file picker",
      "reviewed MIDI snapshot survives external file change",
      "MIDI import preserves other notes/controllers/CASM and byte-exact undo/redo",
      "official Genos PDF catalog import with 1711 verified addresses + invalid import retains cache",
      "catalog persistence across UI reload, search and actual voice confirmation/Cancel/Apply",
      "named voice atomic bank/program with byte-exact undo/redo + Style/GSS reopen",
      "voice edits preserve notes/CASM and stale/unknown voice requests reject",
      "voice preview actions fit minimum window; PDF and prior user catalog restored",
      "independent effect import/search/descriptions, invalid import retains both caches, invalid effect requests reject before MIDI",
      "manual source chord controls and even-tie remap; no document mutations; minimum-window dialogs",
      "actual drum kit selection/Apply via IPC preserves notes/CASM; no physical audition",
      "native Genos per-key scalar UI Cancel/Apply, minimum window, catalog/bypass/stale/unsupported rejection, exact undo/redo and STY/GSS reopen",
      "Style header effects native UI Cancel/Apply, minimum window, bulk retention, catalog/no-catalog return/bypass/stale rejection, exact undo/redo and STY/GSS reopen; no MIDI",
      "Initial effect startup native dry-run/UI default-off, reviewed typed packets, output/document reset and stale/replaced tokens reject before MIDI; no physical send",
    ],
    pageErrors: errors,
    limitations: [
      "Synthetic source fixture and actual Windows IPC/UI; Yamaha playback is not qualified.",
      "No MIDI ports opened or transmitted.",
    ],
  };
  fs.writeFileSync(
    path.join(run, "result.json"),
    JSON.stringify(report, null, 2),
  );
  fs.writeFileSync(
    "../docs/development/reports/latest-source-tracks-run.txt",
    run,
  );
  console.log(`SOURCE TRACKS NATIVE VERIFIED: ${run}`);
  console.log(`CURSOR INTEGRATION NATIVE VERIFIED: ${run}`);
  console.log(`MIDI EXCHANGE NATIVE VERIFIED: ${run}`);
  console.log(`VOICE CATALOG NATIVE VERIFIED: ${run}`);
  console.log(`CURSOR FOLLOWUP NATIVE VERIFIED: ${run}`);
  console.log(`NATIVE DRUM SETUP VERIFIED: ${run}`);
  console.log(`STYLE HEADER EFFECTS VERIFIED: ${run}`);
  console.log(`INITIAL EFFECT PREVIEW VERIFIED: ${run}`);
} catch (error) {
  if (browser) {
    const page = browser.contexts()[0]?.pages()[0];
    if (page) {
      await page
        .screenshot({ path: path.join(run, "failure.png") })
        .catch(() => {});
      fs.writeFileSync(
        path.join(run, "failure.html"),
        await page.content().catch(() => ""),
      );
    }
  }
  throw error;
} finally {
  if (browser) await browser.close();
  if (app && app.exitCode === null && app.signalCode === null) {
    const exited = new Promise((r) => app.once("exit", r));
    app.kill();
    await exited;
  }
  if (vite && vite.exitCode === null && vite.signalCode === null) {
    const exited = new Promise((r) => vite.once("exit", r));
    vite.kill();
    await exited;
  }
  fs.closeSync(log);
  if (recoveryCaptured) {
    for (const [file, bytes] of persistedFiles) {
      if (bytes !== null) fs.writeFileSync(file, bytes);
      else if (fs.existsSync(file)) fs.unlinkSync(file);
    }
  }
}

// Runs after restoring this harness state; never in parallel and never physical MIDI.
execFileSync(process.execPath, ["scripts/live-preview-native.mjs"], {
  stdio: "inherit",
});
