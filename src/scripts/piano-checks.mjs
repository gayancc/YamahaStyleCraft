// Reviewed gate wrappers: success markers are printed only after the complete child exits zero.
import { execFileSync } from "node:child_process";
const mode = process.argv[2];
if (mode === "rust") {
  execFileSync("cargo", ["test", "-p", "genos-core", "--all-targets"], {
    stdio: "inherit",
  });
  console.log("PIANO RUST SUITE VERIFIED");
} else if (mode === "lint") {
  execFileSync(
    "cargo",
    ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    { stdio: "inherit" },
  );
  console.log("PIANO RUST LINT VERIFIED");
} else throw new Error("Expected rust or lint");
