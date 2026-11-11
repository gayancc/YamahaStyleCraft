#!/usr/bin/env node
/**
 * Keep GenosStyleStudio version strings in sync for packaging and GitHub releases.
 *
 * Usage (from src/):
 *   npm run bump -- 0.1.1
 *   node scripts/bump-version.mjs 0.1.1
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+([.-][\w.-]+)?$/.test(version)) {
  console.error("Usage: npm run bump -- <semver>   e.g. npm run bump -- 0.1.1");
  process.exit(1);
}

const root = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(root, "..");

const rewriteJsonVersion = (file) => {
  const p = path.join(srcRoot, file);
  const data = JSON.parse(fs.readFileSync(p, "utf8"));
  data.version = version;
  fs.writeFileSync(p, `${JSON.stringify(data, null, 2)}\n`);
};

const rewriteCargoVersion = (file) => {
  const p = path.join(srcRoot, file);
  const text = fs.readFileSync(p, "utf8");
  const next = text.replace(
    /^version\s*=\s*"[^"]+"/m,
    `version = "${version}"`,
  );
  if (next === text) throw new Error(`No version field found in ${file}`);
  fs.writeFileSync(p, next);
};

rewriteJsonVersion("package.json");
rewriteJsonVersion("src-tauri/tauri.conf.json");
rewriteCargoVersion("src-tauri/Cargo.toml");
rewriteCargoVersion("crates/genos-core/Cargo.toml");

const lockPath = path.join(srcRoot, "package-lock.json");
if (fs.existsSync(lockPath)) {
  const lock = JSON.parse(fs.readFileSync(lockPath, "utf8"));
  lock.version = version;
  if (lock.packages?.[""]) lock.packages[""].version = version;
  fs.writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);
}

console.log(`Version set to ${version}`);
console.log("Next: commit, then tag and push:");
console.log(`  git tag v${version}`);
console.log(`  git push origin v${version}`);
