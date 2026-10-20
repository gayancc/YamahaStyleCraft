// Read-only DevTools inspection of the running desktop webview (launch with --cdp): force pseudo
// states on one element and report the winning declarations. Usage: node scripts/ui-audit-cascade.mjs <selector> <states,...> <props,...>
import { chromium } from "@playwright/test";
const [selector, states = "active,hover", props = "background-image,background-color,color,box-shadow,border-top-color"] = process.argv.slice(2);
const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
try {
  const page = browser.contexts()[0].pages()[0];
  const client = await page.context().newCDPSession(page);
  await client.send("DOM.enable");
  await client.send("CSS.enable");
  const { root } = await client.send("DOM.getDocument", { depth: 0 });
  const { nodeId } = await client.send("DOM.querySelector", { nodeId: root.nodeId, selector });
  if (!nodeId) throw new Error(`no element for ${selector}`);
  await client.send("CSS.forcePseudoState", { nodeId, forcedPseudoClasses: states.split(",") });
  const { computedStyle } = await client.send("CSS.getComputedStyleForNode", { nodeId });
  const want = new Set(props.split(","));
  for (const { name, value } of computedStyle) if (want.has(name)) console.log(`${name}: ${value}`);
  const matched = await client.send("CSS.getMatchedStylesForNode", { nodeId });
  console.log("--- winning rules touching background-image:");
  for (const m of matched.matchedCSSRules) {
    const props2 = m.rule.style.cssProperties.filter((p) => p.name === "background-image" || p.name === "background");
    if (props2.length) console.log(" ", m.rule.selectorList.text.slice(0, 110), "=>", props2.map((p) => `${p.name}:${p.value.slice(0, 50)}`).join(" | "));
  }
  await client.send("CSS.forcePseudoState", { nodeId, forcedPseudoClasses: [] });
} finally {
  await browser.close();
}
