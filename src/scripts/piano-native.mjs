// Real Tauri/WebView2 integration. Never opens a MIDI port or modifies the source fixture.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { chromium, expect } from "@playwright/test";
const run = path.resolve(`../docs/piano-roll/validation/native-${Date.now()}`);
fs.mkdirSync(run, { recursive: true });
const existing = execFileSync(
  "powershell",
  [
    "-NoProfile",
    "-Command",
    "Get-Process -Name genos-style-studio -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id; exit 0",
  ],
  { encoding: "utf8" },
);
assert.equal(
  existing.trim(),
  "",
  "Preserve the running user app; this test needs its own process",
);
execFileSync("cargo", ["build", "--locked", "-p", "genos-style-studio"], {
  stdio: "inherit",
});
for (const port of [1420, 9449]) {
  let busy = false;
  try {
    await fetch(`http://127.0.0.1:${port}`);
    busy = true;
  } catch {}
  assert(!busy, `Port ${port} already in use`);
}
const log = fs.openSync(path.join(run, "process.log"), "wx"),
  persisted = new Map(),
  results = [];
let vite, app, browser, page;
const ready = async (url, child) => {
  for (let i = 0; i < 200; i++) {
    assert.equal(child.exitCode, null);
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw Error(`Timeout ${url}`);
};
try {
  vite = spawn(
    process.execPath,
    [
      "node_modules/vite/bin/vite.js",
      "--host",
      "127.0.0.1",
      "--port",
      "1420",
      "--strictPort",
    ],
    { windowsHide: true, stdio: ["ignore", log, log] },
  );
  await ready("http://127.0.0.1:1420", vite);
  app = spawn(path.resolve("target/debug/genos-style-studio.exe"), [], {
    windowsHide: true,
    stdio: ["ignore", log, log],
    env: {
      ...process.env,
      WEBVIEW2_USER_DATA_FOLDER: path.join(run, "webview-profile"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9449",
    },
  });
  await ready("http://127.0.0.1:9449/json/version", app);
  browser = await chromium.connectOverCDP("http://127.0.0.1:9449");
  page = browser.contexts()[0].pages()[0];
  page.setDefaultTimeout(15000);
  await page.waitForSelector(".brand");
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  const ipc = (cmd, args = {}) =>
    page.evaluate(
      ({ cmd, args }) => window.__TAURI_INTERNALS__.invoke(cmd, args),
      { cmd, args },
    );
  const data = await page.evaluate(async () => {
    const { appDataDir } =
      await import("/node_modules/@tauri-apps/api/path.js");
    return appDataDir();
  });
  for (const name of ["recovery.gss", "recent-files.json", "prefs.json"]) {
    const file = path.join(data, name);
    persisted.set(file, fs.existsSync(file) ? fs.readFileSync(file) : null);
  }
  if (
    await page
      .getByRole("button", { name: "Restore session", exact: true })
      .count()
  )
    await page
      .getByRole("button", { name: "Restore session", exact: true })
      .click();
  const source = path.resolve(
      "../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY",
    ),
    sourceBytes = fs.readFileSync(source),
    sourceHash = crypto.createHash("sha256").update(sourceBytes).digest("hex");
  await page.evaluate(async (source) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(source);
  }, source);
  const state = () =>
    page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      const { view, selected, error, busy } = useStudio.getState();
      return { view, selected, error, busy };
    });
  await expect.poll(async () => (await state()).view?.ppq).toBe(1920);
  const initial = (await state()).view,
    section = initial.sections.find((s) => s.label === "Main A");
  assert(section);
  const initialCasm = initial.routes.map((r) => r.bytes);
  const settle = async () => {
    await expect.poll(async () => (await state()).busy).toBe(false);
    assert.equal((await state()).error, null);
  };
  const edit = async (command) => {
    await page.evaluate(async (command) => {
      const { useStudio } = await import("/frontend/store.ts");
      await useStudio.getState().edit(command);
    }, command);
    await settle();
  };
  const reset = async () => {
    const v = (await state()).view;
    const remove = v.notes
      .filter(
        (n) =>
          n.channel === 11 && n.start >= section.start && n.start < section.end,
      )
      .map((n) => n.id);
    const add = [60, 64, 67].map((pitch, i) => ({
      track: 0,
      channel: 11,
      pitch,
      velocity: 72 + i * 15,
      start: section.start + 960 + i * 960,
      duration: 480,
    }));
    await edit({
      type: "pianoNotes",
      section: section.id,
      channel: 11,
      remove,
      add,
    });
    await page.evaluate(async (id) => {
      const { useStudio } = await import("/frontend/store.ts");
      useStudio.setState({
        sectionId: id,
        channel: 11,
        selected: [],
        tool: "select",
        snap: 16,
        zoom: 1,
      });
    }, section.id);
    await page.evaluate(() => {
      const el = document.querySelector(".piano-scroll");
      el.scrollTop = (127 - 64) * 18 - 120;
      el.scrollLeft = 0;
    });
    await page.waitForTimeout(80);
  };
  const notes = async () => {
    const v = (await state()).view;
    return v.notes.filter(
      (n) =>
        n.channel === 11 && n.start >= section.start && n.start < section.end,
    );
  };
  const coordinate = async (n, edge = "body") =>
    page.evaluate(
      ({ n, section, edge }) => {
        const el = document.querySelector(".piano-scroll"),
          r = el.querySelector("canvas").getBoundingClientRect();
        return {
          x:
            r.x +
            60 +
            ((n.start - section.start) * 72) / 1920 -
            el.scrollLeft +
            (edge === "right"
              ? (n.duration * 72) / 1920 - 2
              : edge === "left"
                ? 1
                : (n.duration * 72) / 1920 / 2),
          y: r.y + 28 + (127 - n.pitch) * 18 - el.scrollTop + 8,
        };
      },
      { n, section, edge },
    );
  const drag = async (a, b) => {
    await page.mouse.move(a.x, a.y);
    await page.mouse.down();
    await page.mouse.move(b.x, b.y, { steps: 8 });
    await page.mouse.up();
    await settle();
  };
  await reset();
  let n = (await notes())[0],
    p = await coordinate(n);
  await drag(p, { x: p.x + 18, y: p.y - 18 });
  let moved = (await notes()).find((m) => m.id === n.id);
  assert.equal(moved.start, n.start + 480);
  assert.equal(moved.pitch, n.pitch + 1);
  results.push("note body move: snap + pitch");
  p = await coordinate(moved, "right");
  await drag(p, { x: p.x + 18, y: p.y });
  let resized = (await notes()).find((m) => m.id === n.id);
  assert.equal(resized.duration, 960);
  results.push("right resize");
  p = await coordinate(resized, "left");
  await drag(p, { x: p.x - 18, y: p.y });
  let left = (await notes()).find((m) => m.id === n.id);
  assert.equal(left.start, resized.start - 480);
  assert.equal(left.duration, resized.duration + 480);
  results.push("left resize retains end");
  await page.keyboard.press("Control+z");
  await settle();
  assert.equal((await notes()).find((m) => m.id === n.id).duration, 960);
  await page.keyboard.press("Control+Shift+z");
  await settle();
  assert.equal((await notes()).find((m) => m.id === n.id).duration, 1440);
  results.push("gesture undo/redo");
  await reset();
  n = (await notes())[0];
  p = await coordinate(n);
  await page.mouse.click(p.x, p.y);
  await page.keyboard.down("Shift");
  const second = await coordinate((await notes())[1]);
  await page.mouse.click(second.x, second.y);
  await page.keyboard.up("Shift");
  assert.equal((await state()).selected.length, 2);
  results.push("additive note selection");
  await page.keyboard.press("Control+ArrowUp");
  await settle();
  assert.equal((await notes()).find((m) => m.id === n.id).pitch, n.pitch + 12);
  results.push("octave shortcut");
  await page.keyboard.press("Control+c");
  await page.keyboard.press("Control+b");
  await settle();
  assert.equal((await notes()).length, 5);
  assert.equal((await state()).selected.length, 2);
  results.push("copy / duplicate selects new notes");
  await page.keyboard.press("Control+x");
  await settle();
  assert.equal((await notes()).length, 3);
  await page.keyboard.press("Control+v");
  await settle();
  assert.equal((await notes()).length, 5);
  results.push("cut / paste scoped notes");
  await reset();
  n = (await notes())[0];
  await page.evaluate(async (id) => {
    const { useStudio } = await import("/frontend/store.ts");
    useStudio.getState().select([id]);
  }, n.id);
  let lane = page.getByLabel("Velocity lane", { exact: true }),
    box = await lane.boundingBox();
  assert(box);
  const x = box.x + 60 + ((n.start - section.start) * 72) / 1920;
  const beforeVelocity = (await state()).view.revision;
  await drag(
    { x, y: box.y + 104 - (n.velocity / 127) * 88 },
    { x, y: box.y + 20 },
  );
  assert.equal((await state()).view.revision, beforeVelocity + 1);
  assert.equal((await notes()).find((m) => m.id === n.id).velocity, 121);
  await page.keyboard.press("Control+z");
  await settle();
  assert.equal((await notes()).find((m) => m.id === n.id).velocity, n.velocity);
  results.push("velocity drag: one transaction and exact undo");
  for (const cc of [7, 11]) {
    await page
      .getByLabel("Controller lane", { exact: true })
      .selectOption(String(cc));
    await page.getByLabel("Controller gesture").selectOption("ramp");
    lane = page.getByLabel(`MIDI CC${cc} lane`, { exact: true });
    box = await lane.boundingBox();
    const revision = (await state()).view.revision;
    await drag(
      { x: box.x + 60, y: box.y + 104 },
      { x: box.x + 132, y: box.y + 16 },
    );
    const v = (await state()).view;
    assert.equal(v.revision, revision + 1);
    const points = v.events.filter(
      (e) =>
        e.data[0] === 0xba &&
        e.data[1] === cc &&
        e.tick >= section.start &&
        e.tick <= section.start + 1920,
    );
    assert(points.some((e) => e.tick === section.start && e.data[2] === 0));
    assert(
      points.some((e) => e.tick === section.start + 1920 && e.data[2] === 127),
    );
    results.push(`CC${cc} explicit MIDI ramp`);
  }
  await page.getByLabel("Controller gesture").selectOption("relative");
  await lane.focus();
  await page.keyboard.press("Control+a");
  const countBefore = (await state()).view.events.filter(
    (e) =>
      e.data[0] === 0xba &&
      e.data[1] === 11 &&
      e.tick >= section.start &&
      e.tick < section.end,
  ).length;
  assert(countBefore > 0);
  await page.keyboard.press("Delete");
  await settle();
  assert(
    !(await state()).view.events.some(
      (e) =>
        e.data[0] === 0xba &&
        e.data[1] === 11 &&
        e.tick >= section.start &&
        e.tick < section.end,
    ),
  );
  results.push("controller multi-selection deletion");
  await page.getByLabel("Controller lane", { exact: true }).selectOption("-1");
  await reset();
  const noteCanvas = page.getByRole("img", { name: /^Piano roll\./ });
  await noteCanvas.focus();
  await page.keyboard.press("Control+a");
  await page.locator(".piano-tool-menu summary").click();
  await page.getByLabel("Note transform").selectOption("gate");
  await page.getByLabel("Transform amount").fill("50");
  await page
    .getByRole("button", { name: "Apply to selection", exact: true })
    .click();
  await settle();
  assert((await notes()).every((n) => n.duration === 240));
  results.push("advanced gate tool");
  await page.locator(".piano-tool-menu summary").click();
  await noteCanvas.focus();
  await page.keyboard.press("b");
  const r = await noteCanvas.boundingBox();
  const y =
    r.y +
    28 +
    (127 - 64) * 18 -
    (await page.locator(".piano-scroll").evaluate((el) => el.scrollTop)) +
    8;
  const startCount = (await notes()).length;
  await drag({ x: r.x + 60 + 144, y }, { x: r.x + 60 + 216, y });
  assert((await notes()).length > startCount + 1);
  results.push("paint gesture adds repeated notes atomically");
  let painted = (await notes()).find(
    (n) => n.start === section.start + 3840 && n.pitch === 64,
  );
  assert(painted);
  let ep = await coordinate(painted);
  const eraseCount = (await notes()).length;
  await page.mouse.click(ep.x, ep.y, { button: "right" });
  await settle();
  assert.equal((await notes()).length, eraseCount - 1);
  results.push("right-click paired-note deletion");
  await reset();
  await noteCanvas.focus();
  await page.keyboard.press("e");
  const marqueeNotes = await notes(),
    top = await coordinate(marqueeNotes[2]),
    bottom = await coordinate(marqueeNotes[0]);
  await drag(
    { x: bottom.x - 15, y: top.y - 10 },
    { x: top.x + 15, y: bottom.y + 14 },
  );
  assert.equal((await state()).selected.length, 3);
  results.push("rectangle selection");
  n = (await notes())[0];
  await page.evaluate(async (id) => {
    const { useStudio } = await import("/frontend/store.ts");
    useStudio.getState().select([id]);
  }, n.id);
  await noteCanvas.focus();
  await page.keyboard.press("Alt+ArrowRight");
  await settle();
  assert.equal((await notes()).find((m) => m.id === n.id).start, n.start + 1);
  results.push("one-tick nudge bypasses snap");
  await page.keyboard.press("p");
  const beforeCancel = (await notes()).length;
  await page.mouse.move(r.x + 330, y);
  await page.mouse.down();
  await page.mouse.move(r.x + 360, y, { steps: 3 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await settle();
  assert.equal((await notes()).length, beforeCancel);
  results.push("Escape cancels draft without mutation");
  await page.mouse.move(r.x + 420, r.y + 150);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(r.x + 350, r.y + 150, { steps: 4 });
  await page.mouse.up({ button: "middle" });
  assert(
    (await page.locator(".piano-scroll").evaluate((el) => el.scrollLeft)) >= 60,
  );
  results.push("middle-button pan");
  await page.locator(".piano-scroll").evaluate((el) => {
    el.scrollLeft = 0;
  });
  const zoomBefore = await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    return { zoom: useStudio.getState().zoom, dpr: devicePixelRatio };
  });
  await page.mouse.move(r.x + 400, r.y + 150);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -100);
  await page.keyboard.up("Control");
  await page.waitForTimeout(100);
  const zoomAfter = await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    return {
      zoom: useStudio.getState().zoom,
      dpr: devicePixelRatio,
      left: document.querySelector(".piano-scroll").scrollLeft,
    };
  });
  assert(zoomAfter.zoom > zoomBefore.zoom);
  assert.equal(zoomAfter.dpr, zoomBefore.dpr);
  assert(
    Math.abs(
      ((400 - 60) * 1920) / (72 * zoomBefore.zoom) -
        ((zoomAfter.left + 400 - 60) * 1920) / (72 * zoomAfter.zoom),
    ) < 40,
  );
  results.push("cursor-anchored zoom without browser zoom");
  const separator = page.getByRole("separator", {
    name: "Controller lane height",
  });
  const oldHeight = Number(await separator.getAttribute("aria-valuenow"));
  await separator.focus();
  await page.keyboard.press("ArrowUp");
  assert.equal(
    Number(await separator.getAttribute("aria-valuenow")),
    oldHeight + 8,
  );
  results.push("accessible lane splitter resize");
  await page.getByRole("button", { name: "Fit", exact: true }).click();
  await page.getByLabel("Pitch row height").focus();
  await page.keyboard.press("Home");
  for (let i = 0; i < 12; i++) await page.keyboard.press("ArrowRight");
  await page.getByLabel("Ghosts", { exact: true }).check();
  await page.getByLabel("Controller lane", { exact: true }).selectOption("7");
  await page.screenshot({ path: path.join(run, "piano-editor.png") });
  results.push("fit / vertical zoom / ghosts / rendered lanes");
  assert.deepEqual(
    (await state()).view.routes.map((r) => r.bytes),
    initialCasm,
  );
  const exportPath = path.join(run, "edited.STY");
  await ipc("save_document", { path: exportPath, projectFile: false });
  const exported = (await state()).view;
  await page.evaluate(async (p) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(p);
  }, exportPath);
  await settle();
  const reopened = (await state()).view;
  const signature = (v) =>
    v.notes.map((n) => [
      n.track,
      n.channel,
      n.pitch,
      n.start,
      n.duration,
      n.velocity,
    ]);
  assert.deepEqual(signature(reopened), signature(exported));
  assert.deepEqual(
    reopened.routes.map((r) => r.bytes),
    initialCasm,
  );
  const midi = (v) =>
    v.events
      .filter((e) => e.data[0] !== 255 || e.data[1] !== 47)
      .map((e) => [e.tick, e.data]);
  assert.deepEqual(midi(reopened), midi(exported));
  assert.equal(
    crypto.createHash("sha256").update(fs.readFileSync(source)).digest("hex"),
    sourceHash,
  );
  results.push(
    "real style save/reopen: note, CC, CASM and unrelated-event identity",
  );
  const dense = Array.from({ length: 10000 }, (_, i) => ({
    track: 0,
    channel: 1,
    pitch: 48 + (i % 24),
    velocity: 70 + (i % 40),
    start: section.start + Math.floor(i / 24) * 48,
    duration: 24,
  }));
  // Measure inside WebView2; repeated full-state CDP snapshots would dominate the result.
  const denseTiming = await page.evaluate(
    async (command) => {
      const { useStudio } = await import("/frontend/store.ts");
      const start = performance.now();
      await useStudio.getState().edit(command);
      const commandMs = performance.now() - start;
      await new Promise((r) =>
        requestAnimationFrame(() => requestAnimationFrame(r)),
      );
      if (useStudio.getState().error) throw Error(useStudio.getState().error);
      return { commandMs, withTwoFramesMs: performance.now() - start };
    },
    { type: "pianoNotes", section: section.id, channel: 1, add: dense },
  );
  const frames = await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    useStudio.setState({ channel: 1, selected: [], zoom: 0.5 });
    await new Promise((r) =>
      requestAnimationFrame(() => requestAnimationFrame(r)),
    );
    const values = [];
    for (const zoom of [0.6, 0.8, 1, 0.7, 0.5]) {
      const t = performance.now();
      useStudio.setState({ zoom });
      await new Promise((r) =>
        requestAnimationFrame(() => requestAnimationFrame(r)),
      );
      values.push(performance.now() - t);
    }
    return values;
  });
  assert(frames.every((t) => t < 1500));
  fs.writeFileSync(
    path.join(run, "performance.json"),
    JSON.stringify(
      {
        notes: 10000,
        denseTiming,
        zoomTwoFrameMs: frames,
        productionBuild: false,
        physicalMidi: false,
      },
      null,
      2,
    ),
  );
  results.push("10,000-note real canvas navigation measured");
  await page.evaluate(async (source) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(source);
    useStudio.setState({ channel: 12, selected: [], zoom: 1, tool: "select" });
  }, source);
  await settle();
  await page.getByLabel("Controller lane", { exact: true }).selectOption("-1");
  await page.getByLabel("Controller gesture").selectOption("draw");
  await page.getByLabel("Ghosts", { exact: true }).uncheck();
  await page.getByRole("button", { name: "Fit", exact: true }).click();
  await page.waitForTimeout(100);
  await page.screenshot({ path: path.join(run, "real-style-editor.png") });
  results.push("representative real style visually rendered");
  // Read pixels from the actual WebView canvas, including black-key tails and
  // the incomplete MIDI endpoints, at both ends of the pitch-zoom range.
  for (const row of [10, 18, 30]) {
    await page.getByLabel("Pitch row height").focus();
    await page.keyboard.press("Home");
    for (let i = 10; i < row; i++) await page.keyboard.press("ArrowRight");
    await page.locator(".piano-scroll").evaluate((el, row) => {
      el.scrollTop = (127 - 66) * row;
    }, row);
    await settle();
    const colors = await page
      .locator(".piano-scroll canvas")
      .evaluate((canvas, row) => {
        const ctx = canvas.getContext("2d"),
          dpr = window.devicePixelRatio || 1;
        const sample = (x, y) =>
          [
            ...ctx.getImageData(Math.floor(x * dpr), Math.floor(y * dpr), 1, 1)
              .data,
          ].slice(0, 3);
        return {
          black: sample(5, 28 + 5.25 * row),
          upperTail: sample(52, 28 + 5.25 * row),
          lowerTail: sample(52, 28 + 5.75 * row),
          white: sample(5, 28 + 6.25 * row),
        };
      }, row);
    assert(colors.black[0] < 80, `black key at ${row}`);
    assert(
      colors.upperTail[0] > 190,
      `full white surface behind black key at ${row}`,
    );
    assert(
      colors.lowerTail[0] < colors.upperTail[0] - 15,
      `C face shading at ${row}`,
    );
    assert(colors.white[0] > 140, `natural key at ${row}`);
    const box = await page.locator(".piano-scroll canvas").boundingBox();
    await page.screenshot({
      path: path.join(run, `keyboard-${row}.png`),
      clip: { x: box.x, y: box.y, width: 160, height: box.height },
    });
    for (const end of ["top", "bottom"]) {
      await page.locator(".piano-scroll").evaluate((el, end) => {
        el.scrollTop = end === "top" ? 0 : el.scrollHeight;
      }, end);
      await settle();
      await page.screenshot({
        path: path.join(run, `keyboard-${row}-${end}.png`),
        clip: { x: box.x, y: box.y, width: 160, height: box.height },
      });
    }
  }
  results.push("keyboard pixels and clipping at 10 / 18 / 30-pixel pitch rows");
  assert.deepEqual(errors, []);
  fs.writeFileSync(
    path.join(run, "result.json"),
    JSON.stringify(
      {
        passed: true,
        source,
        sourceHash,
        hardwareMidiOpened: false,
        checks: results,
      },
      null,
      2,
    ),
  );
  fs.writeFileSync("../docs/piano-roll/validation/latest-native.txt", run);
  console.log(`PIANO NATIVE VERIFIED: ${run}\n${results.length} checks`);
} catch (e) {
  if (page) {
    await page
      .screenshot({ path: path.join(run, "failure.png") })
      .catch(() => {});
    fs.writeFileSync(
      path.join(run, "failure.html"),
      await page.content().catch(() => ""),
    );
  }
  fs.writeFileSync(
    path.join(run, "result.json"),
    JSON.stringify(
      { passed: false, error: String(e), checks: results },
      null,
      2,
    ),
  );
  throw e;
} finally {
  if (browser) await browser.close();
  for (const child of [app, vite])
    if (child && child.exitCode === null && child.signalCode === null) {
      const exited = new Promise((r) => child.once("exit", r));
      child.kill();
      await exited;
    }
  fs.closeSync(log);
  for (const [file, bytes] of persisted) {
    if (bytes !== null) fs.writeFileSync(file, bytes);
    else if (fs.existsSync(file)) fs.unlinkSync(file);
  }
}
