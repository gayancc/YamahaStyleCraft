// Read-only measurement of the running desktop webview (launch with --cdp): contrast and layout of
// whatever is on screen now. Prints one JSON line. Used by ui_audit_sweep.py after it opens a surface.
import { chromium } from "@playwright/test";
import { contrastReport, layoutReport } from "./ui-audit-lib.mjs";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
try {
  const page = browser.contexts()[0].pages()[0];
  const contrast = await contrastReport(page, 4.5);
  const layout = await layoutReport(page);
  const active = contrast.failures.filter((f) => !f.disabled);
  console.log(JSON.stringify({ checked: contrast.checked, contrast: active, layout }));
} finally {
  await browser.close();
}
