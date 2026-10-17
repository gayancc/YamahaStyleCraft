#!/usr/bin/env node
/**
 * Opt-in Genos host listen / MIDI thru pilot (never ordinary CI).
 * Records thru-check + timed listen with midir IOI metrics. Does not close G7.
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
const inName = "Digital Workstation-1";
const outName = "Digital Workstation-1";
const listenSeconds = Number(process.env.GENOS_LISTEN_SECONDS || "8");

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
  session: "listen-thru-pilot",
  device: { input: inName, output: outName },
  listenSeconds,
  steps: {},
  observationChecklist: [
    "Check the selected MIDI input and keyboard transmit routing before listening.",
    "Optional: play keys during the listen window for noteOns > 0.",
    "thru-check sends C4/E4 to Genos and watches host input for echo.",
  ],
  claims: {
    hostListenUxShipped: true,
    wireTimingQualified: false,
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
  report.steps.listen = (
    await run(
      ["listen", inName, String(listenSeconds)],
      (listenSeconds + 30) * 1000,
    )
  ).json;

  report.result = "GENOS_LISTEN_PILOT_COMPLETED";
  report.interpretation = {
    thruEcho:
      (report.steps.thruCheck?.noteOnsSeen ?? 0) > 0
        ? "Input note activity observed; packets were not correlated, so MIDI echo is unverified."
        : "No note input observed; this does not identify the keyboard routing configuration.",
    listenNotes: report.steps.listen?.noteOns ?? 0,
    timingPresent: report.steps.listen?.timing != null,
  };
  report.limitations = [
    "midir timestampUs deltas are driver callbacks, not qualified wire latency.",
    "Zero noteOns records an idle input interval, not a diagnosis of keyboard settings.",
  ];
} catch (e) {
  report.result = "GENOS_LISTEN_PILOT_FAILED";
  report.error = String(e);
}

fs.mkdirSync(docs, { recursive: true });
const dest = path.join(docs, "listen-pilot.json");
fs.writeFileSync(dest, JSON.stringify(report, null, 2));
console.log(report.result);
console.log(`Wrote ${dest}`);
if (report.result !== "GENOS_LISTEN_PILOT_COMPLETED") process.exit(1);
