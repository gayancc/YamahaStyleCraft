// Read-only original workspace inventory. All generated evidence stays here.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
const root = path.resolve("../docs/research");
const reportDir = path.resolve("../docs/development/reports");
fs.mkdirSync(reportDir, { recursive: true });
function walk(dir) {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .flatMap((e) =>
      e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)],
    );
}
const originals = [
  ...[
    "findings",
    "specification",
    "evidence",
    "experiments",
    "tools",
    "artifacts",
  ].flatMap((d) => walk(path.join(root, d))),
  ...fs
    .readdirSync(root)
    .filter((n) => /^00-.*\.md$|^GATES\.md$/.test(n))
    .map((n) => path.join(root, n)),
];
const records = [];
for (const file of originals) {
  const b = fs.readFileSync(file);
  const record = {
    path: path.relative(root, file).replaceAll("\\", "/"),
    bytes: b.length,
    sha256: crypto.createHash("sha256").update(b).digest("hex"),
  };
  if (/\.md$/.test(file)) {
    const text = b.toString("utf8");
    record.headings = text.split(/\r?\n/).filter((l) => /^#{1,3} /.test(l));
    record.classifications = Object.fromEntries(
      ["VERIFIED", "OBSERVED", "INFERRED", "UNKNOWN", "BLOCKED"].map((k) => [
        k,
        text.match(new RegExp(k, "g"))?.length ?? 0,
      ]),
    );
  }
  if (/\.(mjs|js|py|ps1)$/.test(file))
    record.role = b.toString("utf8").split(/\r?\n/).slice(0, 4).join("\n");
  records.push(record);
}
const out = {
  date: "2026-10-16",
  scope:
    "Original research only; inventory/hashing is not a claim to have semantically reviewed all binary/decompiler bytes.",
  records,
};
const dest = path.join(reportDir, "research-inventory.json");
if (process.argv.includes("--check")) {
  const before = JSON.parse(fs.readFileSync(dest, "utf8"));
  for (const r of before.records) {
    const now = records.find((n) => n.path === r.path);
    if (!now || now.sha256 !== r.sha256)
      throw Error(`Original changed: ${r.path}`);
  }
  console.log(`RESEARCH PRESERVED: ${before.records.length} files unchanged`);
} else {
  fs.writeFileSync(dest, JSON.stringify(out, null, 2) + "\n", { flag: "wx" });
  console.log(
    `ADOPTION INVENTORY: ${records.length} originals, ${records.filter((r) => r.headings).length} documents`,
  );
}
