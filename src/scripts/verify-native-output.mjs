import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import assert from "node:assert/strict";
// Retained reports record paths from before the workspace relocation.
const projectRoot = path.resolve("..");
const relocate = (file) => {
  const relative = path.relative(projectRoot, file.trim()).replaceAll("\\", "/");
  if (relative.startsWith("implementation/reports/"))
    return path.join(projectRoot, "docs/development/reports", relative.slice("implementation/reports/".length));
  if (relative.startsWith("experiments/"))
    return path.join(projectRoot, "docs/research", relative);
  return file.trim();
};
const run = relocate(fs.readFileSync("../docs/development/reports/latest-native-run.txt", "utf8"));
const result = JSON.parse(
  fs.readFileSync(path.join(run, "result.json"), "utf8"),
);
const comparison = path.join(run, "independent-event-comparison.json");
if (!fs.existsSync(comparison)) {
  const p = spawnSync(
    process.execPath,
    [
      "../docs/research/tools/compare_style_events.mjs",
      relocate(result.source),
      relocate(result.output),
      comparison,
    ],
    { encoding: "utf8" },
  );
  if (p.status !== 0) throw Error(p.stderr || p.stdout);
}
const c = JSON.parse(fs.readFileSync(comparison, "utf8"));
assert(c.chunks.every((x) => x.left.id === "MTrk" || x.payloadEqual));
const t = c.tracks[0];
assert.equal(t.leftCount, t.rightCount);
assert.equal(t.removed.length, 1);
assert.equal(t.added.length, 1);
assert.deepEqual(t.removed[0].event, [5760, "channel", 0x9a, 36, 72]);
assert.deepEqual(t.added[0].event, [5760, "channel", 0x9a, 36, 73]);
assert.equal(t.removed[0].count, 1);
assert.equal(t.added[0].count, 1);
console.log(
  "INDEPENDENT EXPORT VERIFIED: only the intended velocity changed; header, CASM and all other MIDI events preserved",
);
