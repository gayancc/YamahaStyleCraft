// Explicitly exercises ONLY Microsoft GS Wavetable Synth, never physical ports.
// Synthetic test music is confined to reports. Successful sends do not prove audio.
import { chromium } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9450");
try {
  const page = browser.contexts()[0].pages()[0];
  const ipc = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  const devices = await ipc("scan_devices");
  const port = devices.outputs.find(
    (p) => p.name === "Microsoft GS Wavetable Synth",
  );
  if (!port)
    throw Error("Required software output absent; no fallback port permitted");
  const track = Buffer.from([
    0,
    255,
    81,
    3,
    7,
    161,
    32,
    0,
    255,
    88,
    4,
    4,
    2,
    24,
    8,
    0,
    255,
    6,
    6,
    ...Buffer.from("Main A"),
    0,
    0x9a,
    60,
    72,
    0x81,
    0x70,
    0x8a,
    60,
    0,
    0x81,
    0x70,
    255,
    47,
    0,
  ]);
  const header = Buffer.from([
    77, 84, 104, 100, 0, 0, 0, 6, 0, 0, 0, 1, 1, 224,
  ]);
  const chunk = Buffer.alloc(8);
  chunk.write("MTrk");
  chunk.writeUInt32BE(track.length, 4);
  const file = path.resolve(`../docs/development/reports/software-midi-${Date.now()}.mid`);
  fs.writeFileSync(file, Buffer.concat([header, chunk, track]));
  const v = await ipc("open_document", { path: file });
  assert.equal(v.notes.length, 1);
  assert.equal(v.ppq, 480);
  const args = {
    section: v.sections[0].id,
    channels: [11],
    index: port.index,
    name: port.name,
    looping: false,
  };
  const start = performance.now();
  await ipc("play", args);
  assert.equal((await ipc("transport_status")).playing, true);
  await new Promise((r) => setTimeout(r, 650));
  const completed = await ipc("transport_status");
  assert.equal(completed.playing, false);
  assert.equal(completed.error, null);
  const firstCompletionMs = performance.now() - start;
  await ipc("play", { ...args, looping: true });
  await new Promise((r) => setTimeout(r, 40));
  const cancelStart = performance.now();
  await ipc("stop");
  const cancelMs = performance.now() - cancelStart;
  assert(cancelMs < 500);
  assert.equal((await ipc("transport_status")).playing, false);
  const report = {
    result: "SOFTWARE MIDI ROUTE VERIFIED",
    output: port.name,
    channel: 11,
    notes: 1,
    nominalDurationMs: 500,
    observedCompletionByMs: firstCompletionMs,
    cancelMs,
    sendError: completed.error,
    limitations: [
      "No physical ports opened.",
      "No MIDI loopback/audio capture; sound and precise output latency are unverified.",
      "Cancellation timing measures IPC + worker shutdown on this host, not MIDI wire jitter.",
    ],
  };
  fs.writeFileSync(
    "../docs/development/reports/software-midi-result.json",
    JSON.stringify(report, null, 2),
  );
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
