#!/usr/bin/env node
/**
 * Extended Genos G7 pilot (opt-in, never ordinary CI).
 * Adds input listen + panic_release path. Does not close G7.
 */
import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
execFileSync(
  "cargo",
  ["build", "--locked", "-p", "genos-core", "--example", "midi_probe"],
  { cwd: root, stdio: "inherit" },
);
const docs = path.resolve(root, "../docs/development/reports/genos-hardware");
const sty = path.resolve(
  root,
  "../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
);
const outName = "Digital Workstation-1";
const inName = "Digital Workstation-1";
const sections = ["Main A", "Main B", "Ending A"];

function run(args, timeoutMs = 60000) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      path.join(root, "target/debug/examples/midi_probe.exe"),
      args,
      { cwd: root, shell: false, windowsHide: true },
    );
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error(`timeout: ${args.join(" ")}`));
    }, timeoutMs);
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) {
        reject(
          new Error(`${args.join(" ")} exit ${code}\n${stderr}\n${stdout}`),
        );
        return;
      }
      try {
        const jsonStart = stdout.indexOf("{");
        resolve({
          stderr: stderr.trim(),
          json: jsonStart >= 0 ? JSON.parse(stdout.slice(jsonStart)) : null,
        });
      } catch (error) {
        reject(error);
      }
    });
  });
}

const report = {
  updated: new Date().toISOString(),
  device: { input: inName, output: outName },
  steps: {},
  observationChecklist: [
    "Confirm audible C4/E4 (or section) from baseline-verify notes/section steps.",
    "After panic-check: held G4 should go silent (userAudioObservation).",
    "Optional: play keys during listen window; noteOns should increase if Local Control sends to host.",
  ],
  claims: {
    physicalStuckNoteQualified: false,
    wireTimingQualified: false,
    casmChordAcceptance: false,
    fullStyleAcceptance: false,
    g7Closed: false,
  },
};

try {
  report.steps.scan = (await run(["scan"])).json;
  const outs = report.steps.scan?.outputs?.map((p) => p.name) ?? [];
  const ins = report.steps.scan?.inputs?.map((p) => p.name) ?? [];
  if (!outs.includes(outName)) throw new Error(`missing output ${outName}`);
  if (!ins.includes(inName)) throw new Error(`missing input ${inName}`);

  report.steps.thruCheck = (
    await run(["thru-check", inName, outName], 20000)
  ).json;
  report.steps.listen = (await run(["listen", inName, "5"], 25000)).json;
  report.steps.sections = {};
  for (const label of sections) {
    const step = (await run(["section", sty, label, outName, "2200"], 30000))
      .json;
    report.steps.sections[label] = {
      ...step,
      timingMetrics: {
        wallMs: step?.wallMs ?? step?.elapsedMs ?? null,
        playedUntilMs: step?.playedUntilMs ?? null,
        source: "host wall clock vs section play budget — not wire latency",
      },
    };
  }
  report.steps.panicCheck = (await run(["panic-check", outName], 20000)).json;
  report.steps.releaseCheck = (
    await run(["release-check", outName], 15000)
  ).json;

  report.result = "GENOS_G7_PILOT_COMPLETED";
  report.limitations = [
    "Listen may record zero noteOns if the keyboard is idle or Local Control is off.",
    "Section timingMetrics use host wall clock, not qualified MIDI/audio wire latency.",
    "Panic/release qualify application MIDI sends, not Genos-internal voice engines.",
    "CASM chord preview and full Style acceptance remain evidence-gated.",
    "G7 remains open.",
  ];
} catch (e) {
  report.result = "GENOS_G7_PILOT_FAILED";
  report.error = String(e);
}

fs.mkdirSync(docs, { recursive: true });
const dest = path.join(docs, "g7-pilot.json");
fs.writeFileSync(dest, JSON.stringify(report, null, 2));
console.log(report.result);
console.log(`Wrote ${dest}`);
if (report.result !== "GENOS_G7_PILOT_COMPLETED") process.exit(1);
