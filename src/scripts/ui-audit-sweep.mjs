// Surface sweep of the live Genos window (debug launch with CDP 9449 and Vite; no hardware).
// Opens each inventoried surface through its real control, runs the contrast and layout
// checks, saves a content screenshot and records the result. A surface that cannot be opened
// is recorded as "not-opened" - never as passing.
// Usage: node scripts/ui-audit-sweep.mjs <label>
import { chromium } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { contrastReport, layoutReport } from "./ui-audit-lib.mjs";

const label = process.argv[2] ?? "run";
const outDir = path.resolve(
  `../docs/development/fl-compliance/rea-2026-10-20/sweep-${label}`,
);
fs.mkdirSync(outDir, { recursive: true });
const fixture = path.resolve(
  "../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
);

const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
const results = [];
try {
  const page = browser.contexts()[0].pages()[0];
  await page.reload();
  await page.waitForSelector(".brand");
  await page.evaluate(async (p) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(p);
  }, fixture);
  await page.waitForTimeout(800);
  const viewport = await page.evaluate(() => ({ w: innerWidth, h: innerHeight }));

  const button = (name) => page.getByRole("button", { name, exact: true }).first();
  const menuItem = (menu, name) => async () => {
    await page
      .locator(".studio-menu > summary", {
        hasText: new RegExp(`^\\s*${menu}\\s*$`),
      })
      .first()
      .click();
    await button(name).click();
  };
  const menuOnly = (menu) => async () =>
    page
      .locator(".studio-menu > summary", {
        hasText: new RegExp(`^\\s*${menu}\\s*$`),
      })
      .first()
      .click();
  const click = (name) => async () => button(name).click();
  const surfaces = [
    ["editor", async () => {}],
    ["mixer", click("Mixer")],
    ["inspector-casm", click("CASM")],
    ["inspector-events", click("Events")],
    ["menu-file", menuOnly("File")],
    ["menu-studio", menuOnly("Studio")],
    ["menu-playback-options", menuOnly("Playback options")],
    ["menu-chord-preview", menuOnly("Chord preview")],
    ["menu-view", menuOnly("View")],
    ["menu-note-colors", menuOnly("Note colors")],
    ["expansion-recovery", menuItem("Studio", "Expansion Pack Recovery")],
    ["feature-inventory", menuItem("Studio", "Feature inventory / limits")],
    ["recording-midi", menuItem("Studio", "Recording & MIDI settings")],
    ["help", menuItem("Studio", "Help")],
    ["midi-device-settings", menuItem("Studio", "MIDI device settings")],
    ["library", click("Library")],
    ["section-length", click("Length")],
    ["copy-material", click("Copy")],
    ["voice-picker", click("Voice")],
    ["effect-types", click("Effect types")],
    ["midi-exchange", click("MIDI exchange")],
    ["chord-preview-panel", click("Chord preview")],
    ["tools-window", click("Piano tools")],
  ];
  const reset = async () => {
    for (let i = 0; i < 3; i++) {
      await page.keyboard.press("Escape");
      await page.waitForTimeout(80);
    }
    const piano = page.getByRole("button", { name: "Piano roll", exact: true }).first();
    if (await piano.count()) await piano.click().catch(() => {});
    const notes = page.getByRole("button", { name: "Notes", exact: true }).first();
    if (await notes.count()) await notes.click().catch(() => {});
  };
  for (const [name, open] of surfaces) {
    const entry = { surface: name };
    try {
      await open();
      await page.waitForTimeout(450);
      entry.status = "opened";
      entry.contrast = await contrastReport(page, 4.5);
      entry.layout = await layoutReport(page);
      await page.screenshot({ path: path.join(outDir, `${name}.png`) });
    } catch (e) {
      entry.status = "not-opened";
      entry.error = String(e.message).split("\n")[0].slice(0, 160);
    }
    results.push(entry);
    await reset();
  }
  fs.writeFileSync(
    path.join(outDir, "sweep.json"),
    JSON.stringify({ label, viewport, results }, null, 1),
  );
  console.log(`viewport ${viewport.w}x${viewport.h}`);
  for (const r of results) {
    if (r.status !== "opened") {
      console.log(`  NOT OPENED  ${r.surface}: ${r.error}`);
      continue;
    }
    const active = r.contrast.failures.filter((f) => !f.disabled);
    console.log(
      `  ${active.length || r.layout.length ? "ISSUES    " : "ok        "} ${r.surface}: ` +
        `${r.contrast.checked} text, ${active.length} contrast, ${r.layout.length} layout`,
    );
  }
} finally {
  await browser.close();
}
