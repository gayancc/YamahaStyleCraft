// Drives the real Tauri/WebView2 window (debug launch with CDP 9449 and Vite) for the
// FL compliance audit. Screenshots are taken separately from the desktop with UI Automation,
// so captures include native window chrome. No mock IPC and no hardware sends.
//
// Usage: node scripts/ui-audit-drive.mjs <step> [<step> ...]
//   load                      open the retained ACOUSTIC control fixture through the app store
//   click:<accessible name>   click a button by exact accessible name
//   menu:<label>              open a studio menu disclosure by its summary text
//   key:<Playwright key>      press a key
//   hover:<accessible name>   hover a button
//   wait:<ms>                 wait
//   computed:<selector>|<prop>[,<prop>]  print computed style of the first match
//   reload                    reload the webview
import { chromium } from "@playwright/test";
import path from "node:path";

const fixture = path.resolve(
  "../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
);
// Menu names match whole text ("View" must not match "Chord preview").
const exactText = (text) =>
  new RegExp(`^\\s*${text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*$`);
const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
try {
  const page = browser.contexts()[0].pages()[0];
  for (const step of process.argv.slice(2)) {
    const [kind, ...rest] = step.split(":");
    const arg = rest.join(":");
    if (kind === "reload") {
      await page.reload();
      await page.waitForSelector(".brand");
    } else if (kind === "load") {
      await page.evaluate(async (p) => {
        const { useStudio } = await import("/frontend/store.ts");
        await useStudio.getState().load(p);
        if (useStudio.getState().error) throw Error(useStudio.getState().error);
      }, fixture);
    } else if (kind === "click") {
      await page.getByRole("button", { name: arg, exact: true }).click();
    } else if (kind === "menu") {
      await page
        .locator(".studio-menu > summary", { hasText: exactText(arg) })
        .first()
        .click();
    } else if (kind === "key") {
      await page.keyboard.press(arg);
    } else if (kind === "hover") {
      await page.getByRole("button", { name: arg, exact: true }).hover();
    } else if (kind === "computed") {
      const [selector, props] = arg.split("|");
      console.log(
        JSON.stringify(
          await page.evaluate(
            ({ selector, props }) => {
              const el = document.querySelector(selector);
              if (!el) return null;
              const cs = getComputedStyle(el);
              return Object.fromEntries(
                props.split(",").map((p) => [p, cs.getPropertyValue(p)]),
              );
            },
            { selector, props },
          ),
        ),
      );
    } else if (kind === "wait") {
      await page.waitForTimeout(Number(arg));
    } else {
      throw new Error(`unknown step ${step}`);
    }
  }
  console.log("DRIVE OK");
} finally {
  await browser.close();
}
