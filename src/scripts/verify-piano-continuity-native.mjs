import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { expect } from "@playwright/test";

export async function verifyPianoContinuity({
  page,
  reset,
  notes,
  coordinate,
  state,
  settle,
  results,
  run,
}) {
  await page.evaluate(() => {
    const original = window.fetch;
    window.__continuityRestore = () => {
      window.fetch = original;
    };
    window.__continuityReject = false;
    window.__continuityStarted = 0;
    // Tauri's invoke property is read-only. Intercept its actual fetch transport
    // and assert it is exercised so delay/rejection cannot silently be a no-op.
    window.fetch = async (...args) => {
      const url = args[0] instanceof Request ? args[0].url : String(args[0]);
      if (new URL(url, location.href).pathname === "/edit_document") {
        window.__continuityStarted = (window.__continuityStarted ?? 0) + 1;
        await new Promise((resolve) => setTimeout(resolve, 320));
        if (window.__continuityReject) return new Response("Injected edit rejection", {headers:{"Tauri-Response":"error","Content-Type":"text/plain"}});
      }
      return original(...args);
    };
  });
  const pixel = (point) =>
    page.evaluate((point) => {
      const canvas = document.querySelector(".piano-scroll canvas"),
        box = canvas.getBoundingClientRect();
      return [
        ...canvas
          .getContext("2d")
          .getImageData(
            Math.floor(((point.x - box.x) * canvas.width) / box.width),
            Math.floor(((point.y - box.y) * canvas.height) / box.height),
            1,
            1,
          ).data,
      ];
    }, point);
  const start = (points) =>
    page.evaluate((points) => {
      const canvas = document.querySelector(".piano-scroll canvas"),
        box = canvas.getBoundingClientRect();
      const samples = [],
        originalWidth = Object.getOwnPropertyDescriptor(
          HTMLCanvasElement.prototype,
          "width",
        ),
        originalHeight = Object.getOwnPropertyDescriptor(
          HTMLCanvasElement.prototype,
          "height",
        );
      const capture = { running: true, samples, resizes: 0 };
      window.__continuityCapture = capture;
      for (const [name, descriptor] of [
        ["width", originalWidth],
        ["height", originalHeight],
      ])
        Object.defineProperty(HTMLCanvasElement.prototype, name, {
          ...descriptor,
          set(value) {
            capture.resizes++;
            descriptor.set.call(this, value);
          },
        });
      capture.restore = () => {
        Object.defineProperty(
          HTMLCanvasElement.prototype,
          "width",
          originalWidth,
        );
        Object.defineProperty(
          HTMLCanvasElement.prototype,
          "height",
          originalHeight,
        );
      };
      function frame() {
        if (!capture.running) return;
        samples.push(
          points.map((point) => [
            ...canvas
              .getContext("2d")
              .getImageData(
                Math.floor(((point.x - box.x) * canvas.width) / box.width),
                Math.floor(((point.y - box.y) * canvas.height) / box.height),
                1,
                1,
              ).data,
          ]),
        );
        requestAnimationFrame(frame);
      }
      requestAnimationFrame(frame);
    }, points);
  const stop = () =>
    page.evaluate(() => {
      const c = window.__continuityCapture;
      c.running = false;
      c.restore();
      return { samples: c.samples, resizes: c.resizes };
    });
  const report = [];
  try {
    await reset();
    assert((await page.evaluate(() => window.__continuityStarted)) > 0, "fault injection must intercept real edit IPC");
    let note = (await notes())[0],
      point = await coordinate(note);
    const target = { x: point.x + 18, y: point.y - 18 };
    await start([point, target]);
    await page.mouse.move(point.x, point.y);
    await page.mouse.down();
    await page.mouse.move(target.x, target.y, { steps: 12 });
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    const oldBlank = await pixel(point),
      movedColor = await pixel(target);
    // Discard drag frames: the regression is the release → Rust acknowledgement gap.
    await page.evaluate(() => {
      window.__continuityCapture.samples.length = 0;
    });
    await page.mouse.up();
    await expect.poll(async () => (await state()).busy).toBe(true);
    const opacity = await page
      .locator(".canvas-wrap")
      .evaluate((el) => getComputedStyle(el).opacity);
    assert.equal(opacity, "1", "busy edits must not dim the editor");
    await settle();
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    const move = await stop();
    assert(
      move.samples.length >= 5,
      "sample multiple frames during delayed IPC",
    );
    assert(
      move.samples.every(
        ([old, next]) =>
          JSON.stringify(old) === JSON.stringify(oldBlank) &&
          JSON.stringify(next) === JSON.stringify(movedColor),
      ),
      "released note must never flash back or double-move",
    );
    assert.equal(
      move.resizes,
      0,
      "drag/redraw must not reset canvas backing bitmaps",
    );
    report.push({
      case: "move with 320ms delayed IPC",
      frames: move.samples.length,
      resizes: move.resizes,
      stable: true,
    });

    await reset();
    note = (await notes())[0];
    point = await coordinate(note);
    const blank = await pixel({ x: point.x + 36, y: point.y });
    assert.notDeepEqual(
      await pixel(point),
      blank,
      "positive control: original note is visible",
    );
    await page.evaluate(async (id) => {
      const { useStudio } = await import("/frontend/store.ts");
      useStudio.getState().select([id]);
    }, note.id);
    await page.getByRole("img", { name: /^Piano roll\./ }).focus();
    await page.keyboard.press("Delete");
    await start([point]);
    await settle();
    const deletion = await stop();
    assert(deletion.samples.length >= 5);
    assert(
      deletion.samples.every(
        ([color]) => JSON.stringify(color) === JSON.stringify(blank),
      ),
      "deleted note must stay hidden while IPC is pending",
    );
    assert.equal(deletion.resizes, 0);
    report.push({
      case: "delete with 320ms delayed IPC",
      frames: deletion.samples.length,
      resizes: deletion.resizes,
      stable: true,
    });

    await reset();
    note = (await notes())[0];
    point = await coordinate(note);
    const before = (await state()).view.revision;
    await page.evaluate(() => {
      window.__continuityReject = true;
    });
    await page.mouse.move(point.x, point.y);
    await page.mouse.down();
    await page.mouse.move(point.x + 18, point.y - 18, { steps: 8 });
    await page.mouse.up();
    await expect.poll(async () => (await state()).error).toMatch(/Injected edit rejection/);
    await expect.poll(async () => (await state()).busy).toBe(false);
    assert.match((await state()).error, /Injected edit rejection/);
    assert.equal((await state()).view.revision, before);
    assert.deepEqual(
      (await notes()).find((n) => n.id === note.id),
      note,
    );
    await page.evaluate(() => {
      window.__continuityReject = false;
    });
    report.push({
      case: "rejected move restores original note without history mutation",
      passed: true,
    });
    fs.writeFileSync(
      path.join(run, "interaction-continuity.json"),
      JSON.stringify(report, null, 2),
    );
    results.push(
      "delayed-IPC canvas frames: drag never snaps back, deleted notes never reappear, no bitmap resets or busy dimming; rejected move rolls back",
    );
  } finally {
    fs.writeFileSync(path.join(run, "interaction-continuity.json"), JSON.stringify(report, null, 2));
    await page.evaluate(() => {
      window.__continuityCapture?.restore();
      window.__continuityRestore();
    });
  }
  await reset();
}
