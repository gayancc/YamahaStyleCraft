// Isolated real-app test. Physical MIDI is opened ONLY with explicit --hardware.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { chromium, expect } from "@playwright/test";
const hardware = process.argv.includes("--hardware");
const comparison = process.argv
  .find((a) => a.startsWith("--compare="))
  ?.split("=")[1];
assert(!comparison || hardware, "Comparisons require explicit --hardware");
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
  "Preserve running user app; close it before this isolated check",
);
for (const port of [1420, 9449]) {
  let busy = false;
  try {
    await fetch(`http://127.0.0.1:${port}`);
    busy = true;
  } catch {}
  assert(!busy, `Port ${port} already in use`);
}
execFileSync("cargo", ["build", "--locked", "-p", "genos-style-studio"], {
  stdio: "inherit",
});
const run = path.resolve(
  `../docs/development/reports/live-preview-${hardware ? "hardware" : "native"}-${Date.now()}`,
);
fs.mkdirSync(run, { recursive: true });
const runtimeFiles = [
  "crates/genos-core/src/live_preview.rs",
  "crates/genos-core/src/midi.rs",
  "src-tauri/src/main.rs",
  "frontend/LivePreview.tsx",
  "frontend/App.tsx",
  "target/debug/genos-style-studio.exe",
];
fs.writeFileSync(
  path.join(run, "runtime-sha256.json"),
  JSON.stringify(
    runtimeFiles.map((file) => ({
      path: path.resolve(file),
      sha256: crypto
        .createHash("sha256")
        .update(fs.readFileSync(file))
        .digest("hex"),
    })),
    null,
    2,
  ),
);
const log = fs.openSync(path.join(run, "process.log"), "wx"),
  persisted = new Map();
let vite, app, browser, page, ipc;
const ready = async (url, child) => {
  for (let i = 0; i < 200; i++) {
    assert.equal(child.exitCode, null);
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw Error(`Timeout: ${url}`);
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
  ipc = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    );
  const data = await page.evaluate(async () => {
    const { appDataDir } =
      await import("/node_modules/@tauri-apps/api/path.js");
    return appDataDir();
  });
  for (const name of [
    "recovery.gss",
    "recent-files.json",
    "prefs.json",
    "voice-catalog.json",
    "effect-catalog.json",
  ]) {
    const file = path.join(data, name);
    persisted.set(file, fs.existsSync(file) ? fs.readFileSync(file) : null);
  }
  if (await page.getByText("Discard autosave", { exact: true }).count())
    await page.getByText("Discard autosave", { exact: true }).click();
  const directory = path.resolve("../docs/research/artifacts/style files");
  const source = path.join(
    directory,
    fs
      .readdirSync(directory)
      .find((f) =>
        f.startsWith(
          comparison?.startsWith("drum-") ? "19.LATIN" : "0 M S FERN",
        ),
      ) ?? "MISSING",
  );
  assert(fs.existsSync(source));
  await page.evaluate(async (source) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(source);
  }, source);
  const view = await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    return useStudio.getState().view;
  });
  assert(view);
  const section = view.sections.find((s) => s.label === "Main A");
  assert(section);
  const isDrum = comparison?.startsWith("drum-");
  const auditionSource = comparison === "slash" ? 6 : isDrum ? 10 : 5;
  const typed = async (command, args) => {
    const next = await ipc(command, args);
    await page.evaluate(async (view) => {
      const { useStudio } = await import("/frontend/store.ts");
      useStudio.setState({ view });
    }, next);
    return next;
  };
  const getView = () =>
    page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      return useStudio.getState().view;
    });
  const edit = async (commands) =>
    page.evaluate(async (commands) => {
      const { useStudio } = await import("/frontend/store.ts");
      for (const command of commands) {
        await useStudio.getState().edit(command);
        if (useStudio.getState().error) throw Error(useStudio.getState().error);
      }
    }, commands);
  if (comparison === "effects") {
    const pdf = path.resolve(
      "../docs/development/voice-catalog/genos_en_dl_f0.pdf",
    );
    await ipc("import_effect_catalog", { path: pdf });
    await edit([
      {
        type: "addEvent",
        section: section.id,
        track: 0,
        tick: section.start,
        data: [0xb4, 91, 127],
      },
      {
        type: "addEvent",
        section: section.id,
        track: 0,
        tick: section.start,
        data: [0xb4, 93, 0],
      },
    ]);
    await typed("apply_style_effects", {
      revision: (await getView()).revision,
      changes: [
        { block: "reverb", effect: { msb: 1, lsb: 35 }, returnLevel: 0 },
        { block: "chorus", effect: { msb: 0, lsb: 0 }, returnLevel: 0 },
      ],
    });
  }
  if (isDrum) {
    await ipc("import_voice_catalog", {
      path: path.resolve(
        "../docs/development/voice-catalog/genos_en_dl_f0.pdf",
      ),
    });
    const address = (
      await ipc("drum_channel_status", {
        section: section.id,
        revision: view.revision,
      })
    ).find((d) => d.channel === 10).address;
    assert(address);
    await edit([
      {
        type: "replacePhrase",
        section: section.id,
        channel: 10,
        notes: [0, 1, 2, 3].map((i) => ({
          start: i * view.ppq,
          pitch: 42,
          duration: Math.round(view.ppq / 8),
          velocity: 70,
        })),
      },
    ]);
    await typed("apply_native_drums", {
      revision: (await getView()).revision,
      channel: 10,
      address,
      changes: [
        {
          note: 42,
          field: comparison.slice(5),
          value:
            comparison === "drum-volume"
              ? 20
              : comparison === "drum-pan"
                ? 1
                : -12,
        },
      ],
    });
  }
  const currentView = await getView();
  let port = null;
  if (hardware) {
    const devices = await ipc("scan_devices");
    fs.writeFileSync(
      path.join(run, "devices.json"),
      JSON.stringify(devices, null, 2),
    );
    port = devices.outputs.find((p) => p.name === "Digital Workstation-1");
    assert(port, "Connected Genos MIDI port missing");
    await page.locator(".output-select").click();
    await page
      .getByLabel("MIDI output", { exact: true })
      .selectOption(`${port.index}:${port.name}`);
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Done", exact: true })
      .click();
  }
  if (!hardware) {
    await page.locator(".output-select").click();
    await page.getByLabel("MIDI output", { exact: true }).selectOption("");
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Done", exact: true })
      .click();
  }
  await page.locator("summary").filter({ hasText: /^Playback options$/ }).click();
  await page.getByTitle("Live Style preview", { exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "Live Style preview",
    exact: true,
  });
  if (isDrum) {
    for (const ch of [
      ...new Set(
        view.notes
          .filter(
            (n) =>
              n.start < section.end && n.start + n.duration > section.start,
          )
          .map((n) => n.channel),
      ),
    ].filter((c) => c !== 10))
      await dialog
        .getByLabel(`Preview source ${ch}`, { exact: true })
        .uncheck();
  }
  await dialog
    .getByRole("button", { name: "Check live preview", exact: true })
    .click();
  await expect(dialog.getByText("Preview ready", { exact: true })).toBeVisible({
    timeout: 15000,
  });
  const config = {
    section: section.id,
    sources: [
      ...new Set(
        currentView.notes
          .filter(
            (n) =>
              n.start < section.end && n.start + n.duration > section.start,
          )
          .map((n) => n.channel),
      ),
    ]
      .filter((c) => !isDrum || c === 10)
      .sort((a, b) => a - b),
    chords: [
      { root: 0, quality: "Major", bars: 1 },
      { root: 9, quality: "Minor", bars: 1 },
      { root: 5, quality: "Major", bars: 1 },
      { root: 7, quality: "Major", bars: 1 },
    ],
    effects: false,
    drums: false,
  };
  const preflight = await ipc("preview_live_style", {
    revision: currentView.revision,
    config,
  });
  assert.equal(preflight.report.steps.length, 4);
  assert.equal(preflight.report.routes.length, isDrum ? 1 : 7);
  if (!isDrum) assert(preflight.report.changedPitches > 0);
  fs.writeFileSync(
    path.join(run, "preflight.json"),
    JSON.stringify(
      { source, revision: view.revision, config, ...preflight },
      null,
      2,
    ),
  );
  // Refresh the UI token after the independent native check superseded it.
  await dialog
    .getByRole("button", { name: "Check live preview", exact: true })
    .click();
  await expect(
    dialog.getByText("Preview ready", { exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: path.join(run, "preflight.png") });
  if (hardware) {
    const plays = [];
    const play = async (label) => {
      await dialog
        .getByRole("button", { name: "Check live preview", exact: true })
        .click();
      await expect(
        dialog.getByText("Preview ready", { exact: true }),
      ).toBeVisible();
      console.log(`HARDWARE PLAY START: ${label}`);
      await dialog
        .getByRole("button", { name: "Play progression", exact: true })
        .click();
      await expect(dialog.getByRole("status")).toContainText("Playing", {
        timeout: 5000,
      });
      await page.screenshot({
        path: path.join(run, `playing-${plays.length}.png`),
      });
      // A single 6/8 bar in this Style lasts 2182 ms; progression test has four.
      await page.waitForTimeout(
        comparison ? 2700 : preflight.report.durationMs + 500,
      );
      await dialog.getByRole("button", { name: "Stop", exact: true }).click();
      const status = await ipc("transport_status");
      assert.equal(status.playing, false);
      assert.equal(status.error, null);
      plays.push({ label, status });
    };
    if (comparison) {
      assert(
        [
          "slash",
          "voice",
          "notes",
          "mixer",
          "casm",
          "effects",
          "drum-volume",
          "drum-pan",
          "drum-pitch",
        ].includes(comparison),
      );
      for (const ch of config.sources.filter((c) => c !== auditionSource))
        await dialog
          .getByLabel(`Preview source ${ch}`, { exact: true })
          .uncheck();
      for (const i of [4, 3, 2])
        await dialog
          .getByRole("button", { name: `Remove chord ${i}`, exact: true })
          .click();
      if (comparison === "effects")
        await dialog
          .getByLabel("Initial reverb/chorus type and Return", { exact: true })
          .check();
      if (isDrum)
        await dialog
          .getByLabel("Explicit original drum per-key parameters", {
            exact: true,
          })
          .check();
      await play(
        `${comparison} baseline: isolated source ${auditionSource}, ${comparison === "effects" ? "dry" : "C major"}`,
      );
      await dialog.getByRole("button", { name: "Close", exact: true }).click();
      let commands = [];
      if (comparison === "notes")
        commands = [
          {
            type: "updateNotes",
            section: section.id,
            ids: view.notes
              .filter(
                (n) =>
                  n.channel === 5 &&
                  n.start >= section.start &&
                  n.start < section.end,
              )
              .map((n) => n.id),
            deltaPitch: 12,
            velocity: 40,
          },
        ];
      if (comparison === "voice")
        commands = [
          {
            type: "setVoice",
            section: section.id,
            channel: 5,
            address: { msb: 0, lsb: 0, program: 56 },
          },
        ];
      if (comparison === "mixer")
        commands = [7, 10, 11, 91, 93].map((cc, i) => ({
          type: "addEvent",
          section: section.id,
          track: 0,
          tick: section.start,
          data: [0xb4, cc, [45, 127, 90, 0, 0][i]],
        }));
      if (comparison === "casm")
        commands = [
          {
            type: "sourceTracks",
            section: section.id,
            actions: [
              {
                type: "edit",
                source: 5,
                destination: 12,
                changes: [{ type: "root", value: 7 }],
                chords: null,
                exclusive: false,
              },
            ],
          },
        ];
      await edit(commands);
      if (comparison === "effects")
        await typed("apply_style_effects", {
          revision: (await getView()).revision,
          changes: [{ block: "reverb", effect: null, returnLevel: 127 }],
        });
      if (isDrum) {
        const address = preflight.report.routes[0].voice;
        await typed("apply_native_drums", {
          revision: (await getView()).revision,
          channel: 10,
          address,
          changes: [
            {
              note: 42,
              field: comparison.slice(5),
              value:
                comparison === "drum-volume"
                  ? 110
                  : comparison === "drum-pan"
                    ? 127
                    : 12,
            },
          ],
        });
      }
      const edited = await page.evaluate(async () => {
        const { useStudio } = await import("/frontend/store.ts");
        return useStudio.getState().view;
      });
      fs.writeFileSync(
        path.join(run, "comparison-edit.json"),
        JSON.stringify(
          { comparison, commands, revision: edited.revision },
          null,
          2,
        ),
      );
      await page.locator("summary").filter({ hasText: /^Playback options$/ }).click();
      await page.getByTitle("Live Style preview", { exact: true }).click();
      for (const ch of config.sources.filter((c) => c !== auditionSource))
        await dialog
          .getByLabel(`Preview source ${ch}`, { exact: true })
          .uncheck();
      for (const i of [4, 3, 2])
        await dialog
          .getByRole("button", { name: `Remove chord ${i}`, exact: true })
          .click();
      if (comparison === "effects")
        await dialog
          .getByLabel("Initial reverb/chorus type and Return", { exact: true })
          .check();
      if (isDrum) {
        for (const ch of [
          ...new Set(
            view.notes
              .filter(
                (n) =>
                  n.start < section.end && n.start + n.duration > section.start,
              )
              .map((n) => n.channel),
          ),
        ].filter((c) => c !== 10))
          await dialog
            .getByLabel(`Preview source ${ch}`, { exact: true })
            .uncheck();
        await dialog
          .getByLabel("Explicit original drum per-key parameters", {
            exact: true,
          })
          .check();
      }
      if (comparison === "slash")
        await dialog
          .getByLabel("Chord 1 bass", { exact: true })
          .selectOption("4");
      await play(
        `${comparison} edited: ${comparison === "notes" ? "octave higher, velocity 40" : comparison === "mixer" ? "quieter, hard right, dry sends" : comparison === "effects" ? "wet reverb Return 127" : isDrum ? "drum scalar high/right" : comparison === "slash" ? "C Major/E with NTT Bass enabled on source6" : comparison === "voice" ? "trumpet bank 0/0 program 57" : "CASM source root changed from C to G"}`,
      );
    } else await play("C Major / A Minor / F Major / G Major, routed Main A");
    fs.writeFileSync(
      path.join(run, "playback.json"),
      JSON.stringify(
        {
          port,
          plays,
          comparison,
          observation: "Pending user listening confirmation",
          noUsb: true,
        },
        null,
        2,
      ),
    );
    console.log(
      "HARDWARE PLAY COMPLETE: app progression sent and stopped; listening confirmation pending",
    );
  } else {
    assert(
      await dialog
        .getByRole("button", { name: "Play progression", exact: true })
        .isDisabled(),
    );
    await assert.rejects(
      ipc("play_live_style", {
        token: preflight.token,
        index: 999,
        name: "invalid",
        looping: false,
      }),
      /changed/,
    );
    await assert.rejects(
      ipc("play_live_style", {
        token: "invalid",
        index: 999,
        name: "invalid",
        looping: false,
      }),
      /changed/,
    );
    const fresh = await ipc("preview_live_style", {
      revision: currentView.revision,
      config,
    });
    await edit([
      {
        type: "updateNotes",
        section: section.id,
        ids: [
          view.notes.find(
            (n) =>
              n.channel === 5 &&
              n.start >= section.start &&
              n.start < section.end,
          ).id,
        ],
        velocity: 55,
      },
    ]);
    await assert.rejects(
      ipc("play_live_style", {
        token: fresh.token,
        index: 999,
        name: "invalid",
        looping: false,
      }),
      /changed/,
    );
    await page.evaluate(async (source) => {
      const { useStudio } = await import("/frontend/store.ts");
      await useStudio.getState().load(source);
    }, source);
    await assert.rejects(
      ipc("play_live_style", {
        token: fresh.token,
        index: 999,
        name: "invalid",
        looping: false,
      }),
      /Check live preview before Play$/,
    );
    await dialog
      .getByRole("button", { name: "Check live preview", exact: true })
      .click();
    await expect(
      dialog.getByText("Preview ready", { exact: true }),
    ).toBeVisible();
    await page.setViewportSize({ width: 1000, height: 640 });
    assert(await dialog.evaluate((e) => e.scrollWidth <= e.clientWidth));
    await page.screenshot({ path: path.join(run, "minimum.png") });
    await dialog.getByLabel("Chord 1 bass", { exact: true }).selectOption("4");
    await expect(
      dialog.getByText("Check required", { exact: true }),
    ).toBeVisible();
    await dialog
      .getByRole("button", { name: "Check live preview", exact: true })
      .click();
    await expect(
      dialog.getByText("Preview ready", { exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: path.join(run, "slash-bass-minimum.png") });
    const bassConfig = {
      ...config,
      sources: [6],
      chords: [{ root: 0, quality: "Major", bars: 1 }],
    };
    const bassBaseline = await ipc("preview_live_style", {
      revision: 0,
      config: bassConfig,
    });
    const bassChanged = await ipc("preview_live_style", {
      revision: 0,
      config: {
        ...bassConfig,
        chords: [{ root: 0, quality: "Major", bars: 1, bass: 4 }],
      },
    });
    assert.equal(bassChanged.report.steps[0].label, "C Major/E");
    assert.equal(bassChanged.report.noteOns, bassBaseline.report.noteOns);
    assert(
      bassChanged.report.changedPitches > bassBaseline.report.changedPitches,
    );
    fs.writeFileSync(
      path.join(run, "slash-bass.json"),
      JSON.stringify(
        { baseline: bassBaseline.report, slash: bassChanged.report },
        null,
        2,
      ),
    );
    await assert.rejects(
      ipc("preview_live_style", {
        revision: 0,
        config: {
          ...bassConfig,
          chords: [{ root: 0, quality: "Major", bars: 1, bass: 12 }],
        },
      }),
      /supported chords/,
    );
    await dialog.getByLabel("Chord 1 bass", { exact: true }).selectOption("");
    await expect(
      dialog.getByText("Check required", { exact: true }),
    ).toBeVisible();
    await dialog
      .getByRole("button", { name: "Check live preview", exact: true })
      .click();
    await expect(
      dialog.getByText("Preview ready", { exact: true }),
    ).toBeVisible();
  }
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  if (!hardware) {
    const tables = [];
    for (let table = 0; table <= 10; table++) {
      await edit([
        {
          type: "sourceTracks",
          section: section.id,
          actions: [
            {
              type: "edit",
              source: 6,
              destination: 11,
              changes: [
                { type: "root", value: 0 },
                { type: "chord", value: 0 },
                { type: "zone", low: 0, high: 127 },
                { type: "rule", region: "middle", field: "ntr", value: 0 },
                { type: "rule", region: "middle", field: "ntt", value: table },
              ],
            },
          ],
        },
      ]);
      const state = await getView();
      const checked = await ipc("preview_live_style", {
        revision: state.revision,
        config: { ...config, sources: [6] },
      });
      assert(checked.report.noteOns > 0, `NTT ${table}`);
      tables.push({ table, report: checked.report });
    }
    fs.writeFileSync(
      path.join(run, "ntt-tables.json"),
      JSON.stringify(tables, null, 2),
    );
    console.log("ELEVEN NTT TABLES VERIFIED WITHOUT MIDI");
  }
  fs.writeFileSync(
    path.join(run, "result.json"),
    JSON.stringify(
      { source, hardware, passed: true, report: preflight.report },
      null,
      2,
    ),
  );
  fs.writeFileSync(
    `../docs/development/reports/latest-live-preview-${hardware ? "hardware" : "native"}.txt`,
    run,
  );
  console.log(`LIVE ARRANGER PREVIEW VERIFIED: ${run}`);
  console.log(`SLASH BASS PREVIEW VERIFIED: ${run}`);
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
  throw e;
} finally {
  if (ipc) await ipc("stop").catch(() => {});
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
