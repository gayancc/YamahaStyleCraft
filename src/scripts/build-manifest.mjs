// Package only the exact sources and binaries covered by the current audit.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
const hash = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const json = (file) =>
  JSON.parse(fs.readFileSync(file, "utf8").replace(/^\uFEFF/, ""));
const reportDir = path.resolve("../docs/development/reports");
const audit = json(path.join(reportDir, "cursor-audit.json"));
assert.equal(
  audit.status,
  "PASS",
  "Complete the current integration audit first",
);
const currentFiles = execFileSync(
  "git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
  { cwd: path.resolve(".."), encoding: "utf8" },
)
  .split("\0")
  .filter(Boolean)
  .sort();
assert.deepEqual(
  currentFiles,
  audit.sourceFiles.map((r) => r.path).sort(),
  "Publication file set changed after verification",
);
for (const record of audit.sourceFiles)
  assert.equal(
    hash(fs.readFileSync(path.resolve("..", record.path))),
    record.sha256,
    `Source changed after verification: ${record.path}`,
  );
const artifacts = [
  "target/release/genos-style-studio.exe",
  "target/release/bundle/nsis/GenosStyleStudio_0.1.0_x64-setup.exe",
].map((file) => {
  const bytes = fs.readFileSync(file);
  return { path: path.resolve(file), bytes: bytes.length, sha256: hash(bytes) };
});
const installer = json(path.join(reportDir, "installer-test-final.json"));
assert.equal(
  installer.setupSha256,
  artifacts[1].sha256,
  "Installer evidence belongs to a different build",
);
assert.equal(
  installer.portableSha256,
  artifacts[0].sha256,
  "Application evidence belongs to a different build",
);
const release = path.resolve(
  `../docs/development/releases/audited-${Date.now()}`,
);
fs.mkdirSync(release, { recursive: true });
const manifest = {
  created: new Date().toISOString(),
  status:
    "Audited development build; full feature and Yamaha qualification remain open",
  artifacts,
  release,
  locks: ["Cargo.lock", "package-lock.json"].map((file) => ({
    file,
    sha256: hash(fs.readFileSync(file)),
  })),
  verification: {
    rustTests: audit.rustTests,
    frontendTests: audit.frontendTests,
    gates: audit.gates,
    nativeReport: audit.nativeReport,
  },
  installer,
  hardwarePilot: audit.hardwarePilot,
  historicalMeasurements: {
    render: json(path.join(reportDir, "render-benchmark.json")),
    dependencyAudit: json(path.join(reportDir, "npm-audit-final.json")).metadata
      .vulnerabilities,
  },
  limitations: audit.limitations,
};
for (const artifact of artifacts)
  fs.copyFileSync(
    artifact.path,
    path.join(release, path.basename(artifact.path)),
  );
fs.cpSync(
  "../docs/development/third-party",
  path.join(release, "third-party"),
  { recursive: true },
);
fs.writeFileSync(
  path.join(release, "build-manifest.json"),
  JSON.stringify(manifest, null, 2),
);
fs.writeFileSync(
  path.join(reportDir, "build-manifest.json"),
  JSON.stringify(manifest, null, 2),
);
fs.writeFileSync(
  path.join(release, "README.txt"),
  "GenosStyleStudio development build\n\nRun genos-style-studio.exe with the adjacent third-party folder, or use the x64 installer. WebView2 is required. Source files are protected by new-destination-only saves.\n\nThis is an audited development milestone. Full feature coverage, Yamaha Style/CASM interpretation, recording/clock timing, SFF1 conversion and release qualification remain incomplete. See build-manifest.json and the local development audit for exact scope.\n",
);
console.log(`DEVELOPMENT ARTIFACTS RECORDED: ${release}`);
