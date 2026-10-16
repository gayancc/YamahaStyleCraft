// Integration test against the actual Windows WebView2/Tauri process.
// Requires a debug launch with CDP 9449 and Vite; no mock IPC or hardware sends.
import { chromium, expect } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
const root = path.resolve("../docs/research");
const run = path.resolve(`../docs/development/reports/native-${Date.now()}`);
fs.mkdirSync(run, { recursive: true });
const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
try {
  const page = browser.contexts()[0].pages()[0];
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.reload();
  await page.waitForSelector(".brand");
  if (
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("recovery_status"),
    )
  ) {
    await page
      .getByRole("button", { name: "Restore session", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Restore session", exact: true })
      .waitFor({ state: "hidden" });
  }
  const source = path.join(
    root,
    "experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
  );
  const open = async (p) =>
    page.evaluate(async (p) => {
      const { useStudio } = await import("/frontend/store.ts");
      await useStudio.getState().load(p);
      if (useStudio.getState().error) throw Error(useStudio.getState().error);
    }, p);
  const state = () =>
    page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      return useStudio.getState().view;
    });
  const ipc = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  await open(source);
  let view = await state();
  assert.equal(view.notes.length, 825);
  assert.equal(view.ppq, 1920);
  await page.getByRole("button", { name: /^11 Bass/ }).click();
  const section = view.sections.find((s) => s.label === "Main A");
  const n = view.notes.find(
    (n) => n.channel === 11 && n.start === section.start,
  );
  assert.equal(n.velocity, 72);
  const geometry = await page.evaluate(
    ({ n, section }) => {
      const c = document.querySelector(".piano-scroll canvas"),
        s = document.querySelector(".piano-scroll"),
        r = c.getBoundingClientRect();
      return {
        x:
          r.x +
          60 +
          ((n.start - section.start) * 72) / 1920 -
          s.scrollLeft +
          10,
        y: r.y + 28 + (127 - n.pitch) * 18 - s.scrollTop + 8,
      };
    },
    { n, section },
  );
  await page.mouse.click(geometry.x, geometry.y);
  await page
    .getByRole("spinbutton", { name: "Velocity", exact: true })
    .fill("73");
  await page
    .getByRole("spinbutton", { name: "Velocity", exact: true })
    .press("Tab");
  await expect.poll(async () => (await state()).revision).toBe(1);
  view = await state();
  assert.equal(view.notes.find((x) => x.id === n.id).velocity, 73);
  await page
    .getByRole("button", { name: "Undo (Ctrl+Z)", exact: true })
    .click();
  await expect
    .poll(async () => (await state()).notes.find((x) => x.id === n.id).velocity)
    .toBe(72);
  assert.equal((await state()).notes.find((x) => x.id === n.id).velocity, 72);
  await page
    .getByRole("button", { name: "Redo (Ctrl+Shift+Z)", exact: true })
    .click();
  await expect
    .poll(async () => (await state()).notes.find((x) => x.id === n.id).velocity)
    .toBe(73);
  const output = path.join(run, "edited.STY");
  await ipc("save_document", { path: output, projectFile: false });
  const project = path.join(run, "edited.gss");
  await ipc("save_document", { path: project, projectFile: true });
  await open(project);
  view = await state();
  assert.equal(
    view.notes.find((x) => x.channel === 11 && x.start === 5760).velocity,
    73,
  );
  assert.equal(view.notes.length, 825);
  await page.screenshot({ path: path.join(run, "piano-roll.png") });
  // CASM UI uses native transactions and actual retained route data.
  await page.getByRole("button", { name: /^09 Rhythm 1/ }).click();
  await page.getByRole("button", { name: "CASM", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Route 0 source root" })
    .selectOption("2");
  await expect.poll(async () => (await state()).routes[0].bytes[18]).toBe(2);
  assert.equal((await state()).routes[0].bytes[18], 2);
  await page.screenshot({ path: path.join(run, "casm.png") });
  await page.getByRole("button", { name: "Mixer", exact: true }).click();
  await page.screenshot({ path: path.join(run, "mixer.png") });
  // Explicit scan is read-only and does not open a port or send test notes.
  const devices = await ipc("scan_devices");
  await page.getByRole("button", { name: "Piano roll", exact: true }).click();
  await open(output);
  const final = await state();
  assert.equal(final.notes.length, 825);
  assert.equal(
    final.notes.find((x) => x.channel === 11 && x.start === 5760).velocity,
    73,
  );
  const preserved = fs.readFileSync(source);
  assert.equal(preserved.length, 13061);
  assert.deepEqual(errors, []);
  const report = {
    source,
    output,
    project,
    noteBefore: n,
    noteAfter: final.notes.find((x) => x.channel === 11 && x.start === 5760),
    notes: final.notes.length,
    ppq: final.ppq,
    meter: [final.numerator, final.denominator],
    deviceNames: devices,
    pageErrors: errors,
    result: "NATIVE VERTICAL SLICE VERIFIED",
    limitations: [
      "No MIDI sent; enumeration is not hardware compatibility.",
      "Native dialogs are wired and component-tested; this file integration uses real IPC paths.",
    ],
  };
  fs.writeFileSync(
    path.join(run, "result.json"),
    JSON.stringify(report, null, 2),
  );
  fs.writeFileSync("../docs/development/reports/latest-native-run.txt", run);
  console.log(JSON.stringify(report, null, 2));
} finally {
  await browser.close();
}
