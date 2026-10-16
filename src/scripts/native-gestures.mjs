import { chromium, expect } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
const run = path.resolve(`../docs/development/reports/gestures-${Date.now()}`);
fs.mkdirSync(run, { recursive: true });
const browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
try {
  const page = browser.contexts()[0].pages()[0];
  await page.reload();
  await page.waitForSelector(".brand");
  if (
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("recovery_status"),
    )
  ) {
    await page
      .getByRole("button", { name: "Restore session", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Restore session", exact: true })
      .waitFor({ state: "hidden" });
  }
  const source = path.resolve(
    "../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
  );
  await page.evaluate(async (source) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(source);
  }, source);
  const state = () =>
    page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      const { view, selected, error } = useStudio.getState();
      return { view, selected, error };
    });
  const coordinate = async (n, edge = false) =>
    page.evaluate(
      ({ n, edge }) => {
        const s = document.querySelector(".piano-scroll"),
          c = s.querySelector("canvas"),
          r = c.getBoundingClientRect();
        return {
          x:
            r.x +
            60 +
            ((n.start - 5760) * 72) / 1920 -
            s.scrollLeft +
            (edge ? (n.duration * 72) / 1920 - 2 : 10),
          y: r.y + 28 + (127 - n.pitch) * 18 - s.scrollTop + 8,
        };
      },
      { n, edge },
    );
  await page.getByRole("button", { name: "Piano roll", exact: true }).click();
  await page.getByRole("button", { name: "Notes", exact: true }).click();
  await page.getByRole("button", { name: /^11 Bass/ }).click();
  let { view } = await state();
  const original = view.notes.find((n) => n.channel === 11 && n.start === 5760);
  const get = async () =>
    (await state()).view.notes.find((n) => n.id === original.id);
  const a = await coordinate(original);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(a.x + 18, a.y - 18, { steps: 6 });
  await page.mouse.up();
  await expect.poll(async () => (await get()).start).toBe(6240);
  assert.equal((await get()).pitch, 37);
  assert.equal((await get()).duration, original.duration);
  const moved = await get(),
    r = await coordinate(moved, true);
  await page.mouse.move(r.x, r.y);
  await page.mouse.down();
  await page.mouse.move(r.x + 18, r.y, { steps: 6 });
  await page.mouse.up();
  await expect
    .poll(async () => (await get()).duration)
    .toBe(original.duration + 480);
  // Ctrl+C/Ctrl+V inserts at the most recent edit cursor, preserving relative timing.
  await page.keyboard.press("Control+c");
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await state()).view.notes.length).toBe(826);
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await state()).view.notes.length).toBe(825);
  const firstPoint=await coordinate(await get());
  const second=(await state()).view.notes.find(n=>n.channel===11&&n.start>6240);
  const secondPoint=await coordinate(second);
  await page.mouse.click(firstPoint.x,firstPoint.y);
  await page.keyboard.down('Shift');await page.mouse.click(secondPoint.x,secondPoint.y);await page.keyboard.up('Shift');
  assert.deepEqual(new Set((await state()).selected),new Set([original.id,second.id]));
  const corner={x:Math.min(firstPoint.x,secondPoint.x)-11,y:Math.min(firstPoint.y,secondPoint.y)-10};
  await page.mouse.move(corner.x,corner.y);await page.mouse.down();await page.mouse.move(Math.max(firstPoint.x,secondPoint.x)+20,Math.max(firstPoint.y,secondPoint.y)+12,{steps:5});await page.mouse.up();
  assert((await state()).selected.includes(original.id));assert((await state()).selected.includes(second.id));
  // Clear the marquee, select one note, and explicitly quantize an off-grid edit.
  await page.mouse.click(corner.x,corner.y);await page.mouse.click(firstPoint.x,firstPoint.y);
  const startField=page.getByRole('spinbutton',{name:'Start tick (section)',exact:true});await startField.fill('497');await startField.press('Tab');
  await expect.poll(async()=>(await get()).start).toBe(6257);
  await page.getByRole('button',{name:'Quantize',exact:true}).click();await expect.poll(async()=>(await get()).start).toBe(6240);
  const lane=await page.locator('canvas.velocity').boundingBox();const vp=await coordinate(await get());
  await page.mouse.click(vp.x-9,lane.y+77-83/127*55);await expect.poll(async()=>(await get()).velocity).toBe(83);
  await page.getByRole('button',{name:'Undo (Ctrl+Z)',exact:true}).click();await expect.poll(async()=>(await get()).velocity).toBe(original.velocity);
  // Draw a new high note in empty grid, then remove it using the keyboard.
  await page.evaluate(() => {
    document.querySelector(".piano-scroll").scrollTop = (127 - 90) * 18;
  });
  const c = await page.locator(".piano-scroll canvas").boundingBox();
  await page.mouse.dblclick(c.x + 60 + 36, c.y + 28 + 8);
  await expect.poll(async () => (await state()).view.notes.length).toBe(826);
  let s = await state();
  const added = s.view.notes.find((n) => n.pitch === 90 && n.start === 6720);
  assert(added);
  await page.mouse.click(c.x + 60 + 36 + 8, c.y + 28 + 8);
  await page.keyboard.press("Delete");
  await expect.poll(async () => (await state()).view.notes.length).toBe(825);
  // Actual event authoring and mixer write are native, not local form state.
  await page.getByRole("button", { name: "Events", exact: true }).click();
  const oldEvents = (await state()).view.events.length;
  await page
    .getByRole("button", { name: "Add event at section start" })
    .click();
  await expect
    .poll(async () => (await state()).view.events.length)
    .toBe(oldEvents + 1);
  await page.getByRole("button", { name: "Mixer", exact: true }).click();
  const slider = page.getByRole("slider", {
    name: "Ch 11 volume",
    exact: true,
  });
  const oldVolume = Number(await slider.inputValue());
  await slider.focus();
  await slider.press("ArrowRight");
  await slider.press("Tab");
  await expect
    .poll(
      async () =>
        (await state()).view.events
          .filter(
            (e) => e.tick === 5760 && e.data[0] === 0xba && e.data[1] === 7,
          )
          .at(-1)?.data[2],
    )
    .toBe(oldVolume + 1);
  // Resize the actual native content viewport and verify no horizontal document overflow.
  await page.setViewportSize({width:1000,height:700});
  const metrics = await page.evaluate(() => ({
    width: innerWidth,
    scrollWidth: document.documentElement.scrollWidth,
    canvasWidth: document.querySelector(".piano-scroll canvas")?.clientWidth,
  }));
  assert(metrics.scrollWidth <= metrics.width);
  assert.equal(metrics.width,1000);await page.setViewportSize({width:1480,height:940});
  await page.getByRole("button", { name: "Piano roll", exact: true }).click();
  // Native autosave checkpoint restores actual unsaved content without source cache.
  const checkpoint = await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke("recovery_status"),
  );
  assert.equal(checkpoint, true);
  const restored = await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke("restore_recovery"),
  );
  assert.equal(restored.dirty, true);
  assert.equal(
    restored.notes.find((n) => n.pitch === 37 && n.start === 6240)?.velocity,
    original.velocity,
  );
  // Save a new artifact to clear recovery through the normal native save path.
  const dest = path.join(run, "gesture-result.gss");
  await page.evaluate(
    (path) =>
      window.__TAURI_INTERNALS__.invoke("save_document", {
        path,
        projectFile: true,
      }),
    dest,
  );
  assert.equal(
    await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("recovery_status"),
    ),
    false,
  );
  s = await state();
  assert.equal(s.error, null);
  fs.writeFileSync(
    path.join(run, "result.json"),
    JSON.stringify(
      {
        result: "NATIVE GESTURES VERIFIED",
        checked: [
          "move pitch/time",
          "relative resize",
          "copy/paste",
          "undo",
          "double-click draw",
          "delete",
          "controller event add",
          "mixer CC7 edit",
          "autosave restore",
          "autosave clear on save",
        ],
        metrics,
        original,
        moved: await get(),
        limitations: [
          "Native recovery restore executed in-process; crash/restart test separate.",
        ],
      },
      null,
      2,
    ),
  );
  console.log(
    "NATIVE GESTURES VERIFIED: move, resize, draw, clipboard, delete, event/mixer editing and recovery",
  );
} finally {
  await browser.close();
}
