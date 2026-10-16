// Copies licence notices from installed, locked dependencies. MPL sources remain
// independently licensed and unmodified; no original SFM assets are accessed.
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
const read = (p) =>
  JSON.parse(fs.readFileSync(p, "utf8").replace(/^\uFEFF/, ""));
const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--format-version", "1"], {
  encoding: "utf8",
  maxBuffer: 32 * 1024 * 1024,
}));
fs.mkdirSync("../docs/development/reports", { recursive: true });
fs.writeFileSync("../docs/development/reports/cargo-metadata.json", JSON.stringify(metadata));
const lock = read("package-lock.json");
const dir = path.resolve("../docs/development/third-party");
fs.mkdirSync(dir, { recursive: true });
const records = [];
let notices =
  "# Third-party notices\n\nGenosStyleStudio 0.1.0. Locked dependency inventory, including development/build and non-Windows optional crates for completeness. Original SFM binaries, code, graphics and voice catalogues are not included.\n\n";
const licenseFiles = (d) =>
  fs
    .readdirSync(d, { withFileTypes: true })
    .filter(
      (e) =>
        e.isFile() &&
        /^(licen[cs]e|copying|notice|unlicense)([._-]|$)/i.test(e.name),
    )
    .map((e) => e.name);
for (const p of metadata.packages.filter((p) => p.source)) {
  const base = path.dirname(p.manifest_path),
    files = licenseFiles(base);
  records.push({
    ecosystem: "cargo",
    name: p.name,
    version: p.version,
    license: p.license,
    source: p.repository ?? `https://crates.io/crates/${p.name}/${p.version}`,
    noticeFiles: files,
  });
  notices += `\n## ${p.name} ${p.version} (Rust)\n\nLicence: ${p.license}\nSource: https://crates.io/crates/${p.name}/${p.version}\n`;
  for (const f of files)
    notices += `\n### ${f}\n\n${fs.readFileSync(path.join(base, f), "utf8")}\n`;
  if (p.license === "MPL-2.0") {
    const dest = path.join(dir, "mpl-sources", `${p.name}-${p.version}`);
    fs.cpSync(base, dest, {
      recursive: true,
      filter: (source) => !source.includes(`${path.sep}.git${path.sep}`),
    });
  }
}
for (const [loc, info] of Object.entries(lock.packages)) {
  if (!loc || !fs.existsSync(path.join(loc, "package.json"))) continue;
  const p = read(path.join(loc, "package.json")),
    files = licenseFiles(loc);
  records.push({
    ecosystem: "npm",
    name: p.name,
    version: p.version,
    license: p.license ?? info.license,
    dev: info.dev ?? false,
    source: p.repository ?? null,
    noticeFiles: files,
  });
  notices += `\n## ${p.name} ${p.version} (npm)\n\nLicence: ${p.license ?? info.license ?? "see package notice"}\nSource: https://www.npmjs.com/package/${p.name}/v/${p.version}\n`;
  for (const f of files)
    notices += `\n### ${f}\n\n${fs.readFileSync(path.join(loc, f), "utf8")}\n`;
}
const missing = records.filter((r) => !r.license);
if (missing.length)
  throw Error(`Missing declared licence: ${missing.map((r) => r.name)}`);
fs.writeFileSync(path.join(dir, "NOTICES.md"), notices);
fs.writeFileSync(
  path.join(dir, "dependency-inventory.json"),
  JSON.stringify(records, null, 2),
);
fs.writeFileSync(
  path.join(dir, "README.md"),
  "# Dependency source and notices\n\nNOTICES.md contains upstream licence/copyright notices for locked dependencies. dependency-inventory.json includes declared licences and exact versions. The five MPL-2.0 crate source trees in mpl-sources are included unmodified as an additional source-availability measure. Their licence files govern those files. Other dependencies retain their own licences; where dual permissive licences are offered, this application uses the permissive option. No licence is granted to Yamaha or SFM branding, formats, or content. No such proprietary software is bundled.\n",
);
console.log(
  `NOTICES GENERATED: ${records.length} dependency records; ${records.filter((r) => r.ecosystem === "cargo" && r.license === "MPL-2.0").length} unmodified MPL source trees`,
);
