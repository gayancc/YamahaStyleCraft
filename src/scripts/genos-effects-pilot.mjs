#!/usr/bin/env node
/**
 * Opt-in F021 Genos system-effect hardware pilot (never ordinary CI).
 *
 * A/B audible contrast on Digital Workstation-1:
 *   A) reverb No Effect (0,0) return 0
 *   B) reverb Real Large Hall + (1,35) return 127
 * Plus chorus Celeste vs No Effect. Raises CC91/93, long C4 + tail.
 *
 * Writes docs/development/reports/genos-hardware/effects-pilot.json
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
fs.mkdirSync(docs, { recursive: true });
const outName = "Digital Workstation-1";

function run(args, timeoutMs = 45000) {
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
        reject(new Error(`${args.join(" ")} exit ${code}\n${stderr}\n${stdout}`));
        return;
      }
      const jsonStart = stdout.indexOf("{");
      resolve({
        stderr: stderr.trim(),
        json: jsonStart >= 0 ? JSON.parse(stdout.slice(jsonStart)) : null,
        raw: stdout,
      });
    });
  });
}

const report = {
  updated: new Date().toISOString(),
  feature: "F021",
  device: outName,
  protocol: "A/B No Effect vs Real Large Hall +; CC91/93=127; long C4+tail",
  steps: {},
  claims: {
    sysexFramingVerified: true,
    hardwareSendCompleted: false,
    hardwareAudibleConfirmed: false,
    g6Closed: false,
  },
  observation: {
    priorUserReport: "2026-10-16 mild Hall1→Chorus pilot: user did not hear a clear change",
    listenFor: "A dry/short vs B long hall tail after note release",
  },
};

try {
  report.steps.scan = (await run(["scan"])).json;
  const outs = report.steps.scan?.outputs?.map((p) => p.name) ?? [];
  if (!outs.includes(outName)) {
    throw new Error(`${outName} not present in MIDI outputs`);
  }
  // Catalog-oracle addresses from rust-extract.json (reverb/chorus blocks).
  report.steps.reverbDry = (
    await run(["system-effect", outName, "reverb", "0", "0", "0", "1500"])
  ).json;
  report.steps.reverbWet = (
    await run(["system-effect", outName, "reverb", "1", "35", "127", "1500"])
  ).json;
  report.steps.chorusDry = (
    await run(["system-effect", outName, "chorus", "0", "0", "0", "1200"])
  ).json;
  report.steps.chorusWet = (
    await run(["system-effect", outName, "chorus", "66", "0", "127", "1200"])
  ).json;
  report.claims.hardwareSendCompleted = [
    report.steps.reverbDry,
    report.steps.reverbWet,
    report.steps.chorusDry,
    report.steps.chorusWet,
  ].every((s) => s?.result === "SYSTEM_EFFECT_SEND_COMPLETED");
  report.ok = report.claims.hardwareSendCompleted;
} catch (error) {
  report.ok = false;
  report.error = String(error);
}

const outPath = path.join(docs, "effects-pilot.json");
fs.writeFileSync(outPath, JSON.stringify(report, null, 2));
console.log(
  JSON.stringify(
    {
      reportPath: outPath,
      ok: report.ok,
      claims: report.claims,
      listenFor: report.observation.listenFor,
    },
    null,
    2,
  ),
);
process.exit(report.ok ? 0 : 1);
