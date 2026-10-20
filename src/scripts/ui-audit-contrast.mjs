// Contrast audit of the live Genos window (debug launch with CDP 9449 and Vite).
// For every visible element with its own text, resolve the effective background by blending
// ancestor backgrounds and report WCAG contrast. Disabled controls are reported separately
// because WCAG exempts inactive components.
// Usage: node scripts/ui-audit-contrast.mjs [minRatio=4.5] [--json]
import { chromium } from "@playwright/test";
import { contrastReport } from "./ui-audit-lib.mjs";

const min = Number(process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : 4.5);
const asJson = process.argv.includes("--json");
const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
try {
  const page = browser.contexts()[0].pages()[0];
  const report = await contrastReport(page, min);
  const active = report.failures.filter((f) => !f.disabled);
  const disabled = report.failures.filter((f) => f.disabled);
  if (asJson) console.log(JSON.stringify(report, null, 1));
  else {
    console.log(
      `checked ${report.checked} text elements at >= ${min}:1; ` +
        `${active.length} active failure(s), ${disabled.length} disabled-control exemption(s)`,
    );
    for (const f of active.sort((a, b) => a.ratio - b.ratio))
      console.log(`  ${f.ratio}:1  ${f.selector}  "${f.text}"  ${f.fg} ${f.size}px`);
  }
  process.exitCode = active.length ? 1 : 0;
} finally {
  await browser.close();
}
