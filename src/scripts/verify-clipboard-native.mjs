import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { expect } from "@playwright/test";

export async function verifyClipboardNative({
  page,
  browser,
  run,
  results,
  data,
  persisted,
}) {
  // Empty 1920-PPQ destination with a complete 8-bar 3/4 section.
  const track = [
    0,
    255,
    6,
    6,
    ...Buffer.from("Main A"),
    0,
    255,
    0x58,
    4,
    3,
    2,
    24,
    8,
    0x82,
    0xe8,
    0,
    255,
    47,
    0,
  ]; // 46080 ticks
  const size = Buffer.alloc(4);
  size.writeUInt32BE(track.length);
  const empty = path.join(run, "clipboard-destination-1920.mid");
  fs.writeFileSync(
    empty,
    Buffer.concat([
      Buffer.from([77, 84, 104, 100, 0, 0, 0, 6, 0, 0, 0, 1, 7, 128]),
      Buffer.from("MTrk"),
      size,
      Buffer.from(track),
    ]),
    { flag: "wx" },
  );
  const source = path.join(run, "sfm-equivalent-480.mid");
  const load = async (target, file) =>
    target.evaluate(async (file) => {
      const { useStudio } = await import("/frontend/store.ts");
      await useStudio.getState().load(file);
      useStudio.setState({ channel: 9, selected: [] });
      if (useStudio.getState().error) throw Error(useStudio.getState().error);
    }, file);
  await load(page, source);
  await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    const s = useStudio.getState();
    s.select(
      s.view.notes
        .filter(
          (n) =>
            n.channel === 9 &&
            n.pitch === 51 &&
            (n.start === 0 || n.start === 120),
        )
        .map((n) => n.id),
    );
  });
  await page.getByRole("img", { name: /^Piano roll\./ }).focus();
  await page.keyboard.press("Control+c");
  await expect
    .poll(async () =>
      page.evaluate(
        async () =>
          (await window.__TAURI_INTERNALS__.invoke("read_note_clipboard"))
            ?.notes.length,
      ),
    )
    .toBe(2);
  const label = await page.evaluate(async () => {
    const { openProjectWindow } = await import("/frontend/detach.ts");
    return openProjectWindow();
  });
  const recovery = path.join(data, `recovery-${label}.gss`);
  persisted.set(
    recovery,
    fs.existsSync(recovery) ? fs.readFileSync(recovery) : null,
  );
  let destination;
  await expect
    .poll(() => {
      destination = browser
        .contexts()
        .flatMap((c) => c.pages())
        .find((p) => new URL(p.url()).searchParams.get("doc") === label);
      return Boolean(destination);
    })
    .toBe(true);
  await destination.waitForSelector(".brand", { state: "attached" });
  await expect.poll(() => destination.evaluate(() => window.__TAURI_INTERNALS__.invoke("native_context_menu_enabled"))).toBe(false);
  results.push("new project WebView2 native setting readback: default context menus disabled");
  await load(destination, empty);
  const state = () =>
    destination.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      const s = useStudio.getState();
      return {
        view: s.view,
        busy: s.busy,
        error: s.error,
        clipboardPpq: s.clipboardPpq,
        copied: s.clipboard.length,
      };
    });
  await expect
    .poll(async () => [(await state()).clipboardPpq, (await state()).copied])
    .toEqual([480, 2]);
  await destination.getByRole("img", { name: /^Piano roll\./ }).focus();
  await destination.keyboard.press("Control+v");
  await expect.poll(async () => (await state()).view.notes.length).toBe(2);
  assert.equal((await state()).error, null);
  assert.deepEqual(
    (await state()).view.notes.map((n) => [
      n.start,
      n.duration,
      n.pitch,
      n.velocity,
    ]),
    [
      [0, 280, 51, 98],
      [480, 288, 51, 86],
    ],
  );
  assert.equal((await state()).view.ppq, 1920);
  await destination.keyboard.press("Control+z");
  await expect.poll(async () => (await state()).view.notes.length).toBe(0);
  await destination.keyboard.press("Control+Shift+z");
  await expect.poll(async () => (await state()).view.notes.length).toBe(2);
  assert.deepEqual(
    (await state()).view.notes.map((n) => [n.start, n.duration]),
    [
      [0, 280],
      [480, 288],
    ],
  );
  assert.equal(
    await page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      return useStudio.getState().view.notes.length;
    }),
    96,
  );
  results.push(
    "real project WebViews share source PPQ; 480→1920 clipboard paste scales starts/durations and Undo/Redo preserves source",
  );
  await destination.screenshot({
    path: path.join(run, "clipboard-1920-project.png"),
  });
  // The owning application is closed by the harness; no user window is touched.
}
