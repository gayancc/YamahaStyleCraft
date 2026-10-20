#!/usr/bin/env node
/**
 * Opt-in Genos hardware verification (never ordinary CI).
 * Requires Digital Workstation-1 present. Writes a report JSON under docs/development/reports/genos-hardware/.
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
const docs = path.resolve(process.env.GENOS_VERIFY_REPORT_DIR ?? path.join(root, "../docs/development/reports/genos-hardware"));
const sty = path.resolve(
  root,
  "../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
);
const outName = "Digital Workstation-1";

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
          raw: stdout,
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
  steps: {},
  claims: {
    fullStyleAcceptance: false,
    casmChordAcceptance: false,
    wireTimingQualified: false,
    g7Closed: false,
  },
};

try {
  report.steps.scan = (await run(["scan"])).json;
  const outs = report.steps.scan?.outputs?.map((p) => p.name) ?? [];
  if (!outs.includes(outName)) {
    throw new Error(`${outName} not in outputs: ${outs.join(", ")}`);
  }
  report.steps.notes = (await run(["notes", outName], 20000)).json;
  report.steps.releaseCheck = (
    await run(["release-check", outName], 15000)
  ).json;
  report.steps.section = (
    await run(["section", sty, "Main A", outName, "2500"], 30000)
  ).json;
  report.result = "GENOS_BASELINE_VERIFY_COMPLETED";
} catch (e) {
  report.result = "GENOS_BASELINE_VERIFY_FAILED";
  report.error = String(e);
}

fs.mkdirSync(docs, { recursive: true });
const dest = path.join(docs, "baseline-verify.json");
fs.writeFileSync(dest, JSON.stringify(report, null, 2));
console.log(report.result);
console.log(`Wrote ${dest}`);
if (report.result !== "GENOS_BASELINE_VERIFY_COMPLETED") process.exit(1);
