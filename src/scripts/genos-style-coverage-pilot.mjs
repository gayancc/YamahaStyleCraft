#!/usr/bin/env node
/**
 * Opt-in: play every musical section in the Acoustic fixture (short clips).
 * Does not close full Style acceptance / G7.
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
const maxMs = "1800";

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
  device: outName,
  stylePath: sty,
  steps: {},
  claims: {
    multiSectionAudiblePilot: false,
    fullStyleAcceptance: false,
    wireTimingQualified: false,
    g7Closed: false,
  },
};

try {
  report.steps.sections = (await run(["sections", sty])).json;
  const labels = (report.steps.sections?.sections ?? []).map((s) => s.label);
  if (!labels.length) throw new Error("no sections in style");
  report.steps.playback = {};
  for (const label of labels) {
    report.steps.playback[label] = (
      await run(["section", sty, label, outName, maxMs], 25000)
    ).json;
  }
  report.result = "GENOS_STYLE_COVERAGE_PILOT_COMPLETED";
  report.sectionCountPlayed = labels.length;
  report.limitations = [
    "Short clips per section — not full-length Style audition.",
    "Audible result requires user observation.",
    "Does not claim CASM chord preview or wire-qualified timing.",
  ];
} catch (e) {
  report.result = "GENOS_STYLE_COVERAGE_PILOT_FAILED";
  report.error = String(e);
}

fs.mkdirSync(docs, { recursive: true });
const dest = path.join(docs, "style-coverage-pilot.json");
fs.writeFileSync(dest, JSON.stringify(report, null, 2));
console.log(report.result);
console.log(`sections=${report.sectionCountPlayed ?? 0} Wrote ${dest}`);
if (report.result !== "GENOS_STYLE_COVERAGE_PILOT_COMPLETED") process.exit(1);
