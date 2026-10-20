import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { expect } from "@playwright/test";

// Runs in the existing isolated Tauri harness, with real IPC and no hardware sends.
export async function verifyUiCompliance({ page, run, results, source }) {
  const originalViewport = page.viewportSize();
  const captures = [],
    layouts = [];
  const button = (name) => page.getByRole("button", { name, exact: true });
  const state = () =>
    page.evaluate(async () => {
      const { useStudio } = await import("/frontend/store.ts");
      const s = useStudio.getState();
      return {
        revision: s.view?.revision,
        notes: s.view?.notes,
        routes: s.view?.routes,
        tool: s.tool,
      };
    });
  const capture = async (name) => {
    const file = `compliance-${name}.png`;
    await page.screenshot({ path: path.join(run, file) });
    captures.push(file);
  };
  const studio = async (name) => {
    await page.locator(".titlebar-tools .studio-menu > summary").click();
    await button(name).click();
  };
  const bounds = async (panel, name) => {
    const r = await panel.boundingBox(),
      viewport = await page.evaluate(() => ({
        width: innerWidth,
        height: innerHeight,
      }));
    assert(
      r &&
        r.x >= 0 &&
        r.y >= 0 &&
        r.x + r.width <= viewport.width + 1 &&
        r.y + r.height <= viewport.height + 1,
      `${name} stays in viewport: ${JSON.stringify(r)}`,
    );
    assert(
      await panel.evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
      `${name} has no horizontal content clipping`,
    );
    layouts.push({ name, viewport, rect: r });
  };
  const inspectDialog = async (name, slug, modal = true) => {
    const panel = page.getByRole("dialog", { name, exact: true });
    await expect(panel).toBeVisible();
    await bounds(panel, name);
    if (modal) {
      assert.equal(await panel.getAttribute("aria-modal"), "true");
      await expect
        .poll(() => panel.evaluate((el) => el.contains(document.activeElement)))
        .toBe(true);
      const ends = await panel.evaluate(async (el) => {
        const { focusableControls } = await import("/frontend/modal.ts");
        const controls = focusableControls(el);
        controls.at(-1)?.focus();
        return controls.length;
      });
      assert(ends > 0, `${name} has reachable controls`);
      await page.keyboard.press("Tab");
      assert(
        await panel.evaluate(async (el) => {
          const { focusableControls } = await import("/frontend/modal.ts");
          return document.activeElement === focusableControls(el)[0];
        }),
        `${name} wraps Tab`,
      );
      await page.keyboard.press("Shift+Tab");
      assert(
        await panel.evaluate(async (el) => {
          const { focusableControls } = await import("/frontend/modal.ts");
          return document.activeElement === focusableControls(el).at(-1);
        }),
        `${name} wraps Shift+Tab`,
      );
    }
    await capture(slug);
    await panel.click({ position: { x: 5, y: 5 } });
    // Focus a real control before dismissing so this tests the actual bubbling path.
    await panel.evaluate(async (el) => {
      const { focusableControls } = await import("/frontend/modal.ts");
      focusableControls(el)[0]?.focus();
    });
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden();
    results.push(
      `${name}: viewport bounds, visible controls, ${modal ? "bidirectional focus trap, " : ""}Escape dismissal`,
    );
  };
  for (const [width, height] of [
    [1000, 700],
    [1480, 900],
  ]) {
    await page.setViewportSize({ width, height });
    await page.evaluate(async (source) => {
      const { useStudio } = await import("/frontend/store.ts");
      await useStudio.getState().load(source);
      useStudio.setState({ channel: 11, selected: [] });
    }, source);
    await button("Notes").click();
    await button('Next note').click();
    await page.locator('.note-effects-disclosure > summary').click();
    const effects = page.locator('.note-fx-panel');
    await effects.getByLabel('Note effect controller', { exact: true }).selectOption('91');
    await effects.getByRole('button', { name: 'Enter Note effect start value', exact: true }).click();
    const startValue = effects.getByRole('spinbutton', { name: 'Note effect start value', exact: true });
    await startValue.fill('36'); await startValue.press('Enter');
    await capture(`note-effects-${width}`);
    const eventsBeforeFx = await page.evaluate(async () => {
      const { useStudio } = await import('/frontend/store.ts'); return JSON.stringify(useStudio.getState().view.events);
    });
    const revisionBeforeFx = (await state()).revision;
    await effects.getByRole('button', { name: 'Apply to selected notes', exact: true }).click();
    await expect.poll(async () => (await state()).revision).toBe(revisionBeforeFx + 1);
    await button('Undo (Ctrl+Z)').click();
    await expect.poll(() => page.evaluate(async () => {
      const { useStudio } = await import('/frontend/store.ts'); return JSON.stringify(useStudio.getState().view.events);
    })).toBe(eventsBeforeFx);
    results.push(`${width}: Note effects exact entry and native CC-envelope Apply undo to the original ordered events`);
    const before = await state();
    const suffix = `${width}`;
    await button("Next note").click();
    const pitch = page.getByRole("spinbutton", { name: "Pitch", exact: true });
    const originalPitch = await pitch.inputValue();
    await pitch.fill(originalPitch === "120" ? "119" : "120");
    await pitch.press("Escape");
    assert.equal(await pitch.inputValue(), originalPitch);
    assert.equal(
      (await state()).revision,
      before.revision,
      "Escape cancels precise inspector entry",
    );

    // Menu keys must never nudge notes or change tools in the underlying piano.
    const summary = page.locator(".file-actions .studio-menu > summary");
    await summary.focus();
    await summary.press("ArrowDown");
    await expect(button("Open")).toBeFocused();
    await page.keyboard.press("End");
    await expect(button("Export section JSON snapshot")).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(summary).toBeFocused();
    assert.deepEqual(await state(), before);

    for (const [command, name, slug] of [
      ["Help", "Help", "help"],
      ["Feature inventory / limits", "Feature inventory", "inventory"],
      ["Expansion Pack Recovery", "Expansion Pack Recovery", "expansion"],
      ["Recording & MIDI settings", "Recording & MIDI", "recording"],
    ]) {
      await studio(command);
      await inspectDialog(name, `${slug}-${suffix}`);
    }
    await page.locator(".transport > .studio-menu > summary").click();
    await button("Live").click();
    await inspectDialog("Live Style preview", `live-${suffix}`);
    await page.locator(".file-actions .studio-menu > summary").click();
    await button("New").click();
    await inspectDialog("Style Composer", `composer-${suffix}`);
    await page.locator(".status-action").click();
    await inspectDialog("File integrity", `integrity-${suffix}`);
    for (const [command, name, slug] of [
      ["Copy", "Copy material", "copy"],
      ["Length", "Shorten Section", "length"],
      ["Library", "Style Part Library", "library"],
      ["MIDI exchange", "MIDI exchange", "exchange"],
      ["Chord preview", "Chord preview", "chord"],
      ["Effect types", "Genos effect types", "effects"],
    ]) {
      await button(command).click();
      await inspectDialog(name, `${slug}-${suffix}`);
    }
    await button("Browse Genos voices").click();
    await expect(
      page.locator(".voice-picker-window .voice-results button").first(),
    ).toBeVisible();
    await inspectDialog("Genos voices", `voices-${suffix}`, false);
    await button("CASM").click();
    await capture(`casm-${suffix}`);
    await button("Edit source tracks").first().click();
    await inspectDialog("Source tracks", `sources-${suffix}`);
    await button("Events").click();
    await capture(`events-${suffix}`);
    await button("Notes").click();
    await page.getByRole("button", { name: /^09 Rhythm 1/ }).click();
    await button("Drum kits (Ch 9–10)").click();
    const drums = page.getByRole("dialog", { name: "Drum kits", exact: true });
    await expect(drums).toBeVisible();
    await bounds(drums, "Drum kits");
    await capture(`drums-${suffix}`);
    await drums
      .getByRole("button", { name: "Genos per-key setup…", exact: true })
      .click();
    await inspectDialog("Genos per-key Drum Setup", `drum-setup-${suffix}`);

    await page.locator(".output-select").click();
    await inspectDialog("MIDI connections", `midi-${suffix}`, false);
    await button("Piano tools").click();
    const transform = page.getByRole("combobox", {
      name: "Note transform",
      exact: true,
    });
    for (const value of await transform
      .locator("option")
      .evaluateAll((els) => els.map((el) => el.value))) {
      await transform.selectOption(value);
      await bounds(
        page.locator(".floating-tool-window:not([hidden])"),
        `Transform ${value}`,
      );
    }
    await transform.selectOption("quantize");
    await inspectDialog("Quantize", `quantize-${suffix}`, false);
    await expect(button("Piano tools")).toBeFocused();
    const lane = page.getByRole("combobox", {
      name: "Controller lane",
      exact: true,
    });
    for (const value of await lane
      .locator("option")
      .evaluateAll((els) => els.map((el) => el.value))) {
      await lane.selectOption(value);
      await expect(page.locator(".controller-lane canvas")).toBeVisible();
    }
    await capture(`controller-lanes-${suffix}`);
    await lane.selectOption("-1");
    await capture(`piano-${suffix}`);
    const palette = page.locator('.note-colors-menu');
    await palette.locator('summary').click();
    await expect(palette.getByRole('group', { name: 'Note colors' })).toBeVisible();
    assert.equal(await palette.locator('input[type="color"]').count(), 16);
    await bounds(palette.locator('.studio-menu-content'), 'Note colors');
    await capture(`palette-${suffix}`);
    await palette.locator('summary').press('Escape');
    await expect(palette).not.toHaveAttribute('open', '');
    await button("Mixer").click();
    for (const mode of ["Sends", "Tone / EQ", "Performance"]) {
      await button(mode).click();
      await capture(`mixer-${mode.replace(/\W/g, "")}-${suffix}`);
    }
    await button("Piano roll").click();
    for (const track of await page.locator(".track-select").all())
      await track.click();
    for (const section of await page
      .locator(".sections-strip nav button")
      .all())
      await section.click();
    await button("Hide inspector").click();
    await expect(button("Show inspector")).toBeVisible();
    await capture(`collapsed-inspector-${suffix}`);
    await button("Show inspector").click();
    assert.equal(
      (await state()).revision,
      before.revision,
      "inspection and draft cancellation do not edit the Style",
    );
    assert.deepEqual(
      (await state()).routes,
      before.routes,
      "UI inspection preserves CASM",
    );
    assert.deepEqual(
      (await state()).notes,
      before.notes,
      "UI inspection preserves notes",
    );
    results.push(
      `${width}x${height}: menus, all inspectors, all mixer modes, utility windows and Yamaha draft cancellation preserve notes/CASM`,
    );
  }
  await page.setViewportSize({ width: 1000, height: 700 });
  for (const [delta, count, scale] of [
    ["Increase", 5, 125],
    ["Decrease", 8, 85],
  ]) {
    await studio("Feature inventory / limits");
    const inventory = page.getByRole("dialog", {
      name: "Feature inventory",
      exact: true,
    });
    for (let i = 0; i < count; i++)
      await button(`${delta} interface size`).click();
    await bounds(inventory, `Interface scale ${scale}%`);
    await page.keyboard.press("Escape");
    const footer = await page.locator(".statusbar").boundingBox();
    assert(
      footer.y + footer.height <= 701,
      `statusbar remains in viewport at ${scale}%: ${JSON.stringify(footer)}`,
    );
    const grid = page.locator('.piano-scroll');
    await grid.scrollIntoViewIfNeeded();
    const gridRect = await grid.boundingBox();
    assert(gridRect.height >= 160, `usable pitch grid at ${scale}%: ${JSON.stringify(gridRect)}`);
    await capture(`interface-scale-${scale}`);
    await page.getByRole('combobox', { name: 'Controller lane', exact: true }).scrollIntoViewIfNeeded();
    await expect(page.getByRole('combobox', { name: 'Controller lane', exact: true })).toBeInViewport();
  }
  await studio("Feature inventory / limits");
  await button("Reset interface size to 100%").click();
  await page.keyboard.press("Escape");
  await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    useStudio.setState({ view: null });
  });
  await expect(button("Open Yamaha Style")).toBeVisible();
  await capture("empty");
  await page.evaluate(async (source) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(source);
  }, source);
  const restoredContent = await page.evaluate(async () => {
    const { useStudio } = await import('/frontend/store.ts');
    const s = useStudio.getState(), section = s.view.sections.find(x => x.label === 'Main A');
    const note = s.view.notes.find(x => x.channel === 11 && x.start >= section.start && x.start < section.end);
    await s.edit({ type: 'updateNotes', section: section.id, ids: [note.id], velocity: note.velocity === 127 ? 126 : note.velocity + 1 });
    if (useStudio.getState().error) throw Error(useStudio.getState().error);
    const v = useStudio.getState().view;
    return { notes: v.notes.map(({ channel, pitch, velocity, start, duration, track }) => ({ channel, pitch, velocity, start, duration, track })), routes: v.routes.map(x => x.bytes) };
  });
  await page.reload(); await page.waitForSelector('.brand');
  const recovery = page.getByRole('dialog', { name: 'Restore your last session?', exact: true });
  await expect(recovery).toBeVisible(); await bounds(recovery, 'Session recovery');
  await page.keyboard.press('Escape'); await expect(recovery).toBeVisible();
  await capture('session-recovery');
  await recovery.getByRole('button', { name: 'Restore session', exact: true }).click();
  await expect(recovery).toHaveCount(0);
  assert.deepEqual(await page.evaluate(async () => {
    const { useStudio } = await import('/frontend/store.ts'); const v = useStudio.getState().view;
    return { notes: v.notes.map(({ channel, pitch, velocity, start, duration, track }) => ({ channel, pitch, velocity, start, duration, track })), routes: v.routes.map(x => x.bytes) };
  }), restoredContent);
  results.push('Recovery prompt rendered after a real autosave; Escape preserves the choice and Restore retains edited notes/CASM');
  await page.evaluate(async source => {
    const { useStudio } = await import('/frontend/store.ts'); await useStudio.getState().load(source);
  }, source);
  fs.writeFileSync(
    path.join(run, "ui-compliance.json"),
    JSON.stringify(
      { passed: true, captures, layouts, hardwareMidiOpened: false },
      null,
      2,
    ),
  );
  if (originalViewport) await page.setViewportSize(originalViewport);
}
