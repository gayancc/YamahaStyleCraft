import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { expect } from "@playwright/test";

// Invoked only by the isolated native harness. No MIDI ports are opened.
export async function verifyQuantizeNative({ page, ipc, run, results }) {
  const oracle = JSON.parse(fs.readFileSync("crates/genos-core/tests/fixtures/sfm-quantize-dense-eighth-oracle.json", "utf8"));
  const vlq = (n) => {
    const bytes = [n & 127];
    while ((n = Math.floor(n / 128))) bytes.unshift((n & 127) | 128);
    return bytes;
  };
  const chunk = (tag, bytes) => {
    const size = Buffer.alloc(4);
    size.writeUInt32BE(bytes.length);
    return Buffer.concat([Buffer.from(tag), size, Buffer.from(bytes)]);
  };
  const events = [
    { tick: 0, data: [255, 6, 6, ...Buffer.from("Main A")] },
    { tick: 0, data: [255, 0x58, 4, 3, 2, 24, 8] },
    { tick: 0, data: [255, 0x51, 3, 6, 253, 37] },
  ];
  for (const n of oracle.notes) {
    events.push({ tick: n.beforeStart, data: [0x98, n.pitch, n.velocity] });
    events.push({ tick: n.beforeStart + n.beforeDuration, data: [0x88, n.pitch, 0] });
  }
  events.push({ tick: oracle.sectionLength, data: [255, 47, 0] });
  events.sort((a, b) => a.tick - b.tick);
  let last = 0;
  const track = events.flatMap((e) => {
    const bytes = [...vlq(e.tick - last), ...e.data];
    last = e.tick;
    return bytes;
  });
  const fixture = path.join(run, "sfm-equivalent-480.mid");
  const input = Buffer.concat([
    chunk("MThd", [0, 0, 0, 1, 1, 224]),
    chunk("MTrk", track),
    chunk("TEST", [17, 22, 33, 44]),
  ]);
  fs.writeFileSync(fixture, input, { flag: "wx" });
  const load = async (file) => page.evaluate(async (file) => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().load(file);
    useStudio.setState({ channel: 9, selected: [] });
  }, file);
  const state = () => page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    const s = useStudio.getState();
    return { view: s.view, error: s.error, busy: s.busy, snap: s.snap };
  });
  const settle = async () => {
    await expect.poll(async () => (await state()).busy).toBe(false);
    assert.equal((await state()).error, null);
  };
  const timing = (view) => view.notes.map((n) => [n.pitch, n.velocity, n.start, n.duration])
    .sort((a, b) => a[2] - b[2] || a[0] - b[0] || a[1] - b[1]);
  const expected = (prefix) => oracle.notes.map((n) => [n.pitch, n.velocity, n[`${prefix}Start`], n[`${prefix}Duration`]])
    .sort((a, b) => a[2] - b[2] || a[0] - b[0] || a[1] - b[1]);
  await load(fixture);
  await settle();
  assert.equal((await state()).view.ppq, 480);
  assert.deepEqual(timing((await state()).view), expected("before"));
  // The fixture carries the actual 3/4 meter rather than SFM's misleading header.
  assert.equal((await state()).view.numerator, 3);
  if (!(await page.getByLabel("Note transform",{exact:true}).isVisible())) await page.getByRole("button",{name:"Piano tools",exact:true}).click();
  await page.getByLabel("Note transform", { exact: true }).selectOption("quantize");
  await page.getByLabel("Quantize size", { exact: true }).selectOption("240");
  assert.equal((await state()).snap, 8);
  await page.getByRole("button", { name: "Quantize whole part", exact: true }).click();
  await settle();
  let quantized = (await state()).view;
  assert.deepEqual(timing(quantized), expected("after"));
  assert.equal(quantized.notes.length, 96);
  assert(quantized.canUndo);
  assert(quantized.quantizeOriginal);
  results.push("advanced quantize UI: all 96 notes match the saved live SFM dense-eighth output at equivalent PPQ");
  const project = path.join(run, "quantized.gss");
  await ipc("save_document", { path: project, projectFile: true, docLabel: "main" });
  await load(project);
  await settle();
  quantized = (await state()).view;
  assert.deepEqual(timing(quantized), expected("after"));
  assert.equal(quantized.dirty, false);
  await page.getByRole("button", { name: "Reset original", exact: true }).click();
  await settle();
  assert.deepEqual(timing((await state()).view), expected("before"));
  assert.equal((await state()).view.quantizeOriginal, undefined);
  await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().history(false);
  });
  await settle();
  assert.deepEqual(timing((await state()).view), expected("after"));
  assert.equal((await state()).view.dirty, false);
  await page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    await useStudio.getState().history(true);
  });
  await settle();
  assert.deepEqual(timing((await state()).view), expected("before"));
  results.push("quantize native project save/reopen, stable event identities, Reset and atomic Undo/Redo");
  const exported = path.join(run, "reset-output.mid");
  await ipc("save_document", { path: exported, projectFile: false, docLabel: "main" });
  const output = fs.readFileSync(exported);
  assert(output.includes(chunk("TEST", [17, 22, 33, 44])));
  await load(exported);
  await settle();
  assert.deepEqual(timing((await state()).view), expected("before"));
  assert.deepEqual(fs.readFileSync(fixture), input);
  results.push("reset MIDI save/reopen preserves all original note timing, source bytes and opaque chunks");
  await page.screenshot({ path: path.join(run, "advanced-quantize.png") });
  const bounds=[];
  for(const [width,height] of [[1000,640],[1480,900]]) {
    await page.setViewportSize({width,height});
    if(!(await page.getByLabel("Note transform",{exact:true}).isVisible())) await page.getByRole("button",{name:"Piano tools",exact:true}).click();
    const frame=page.getByRole("dialog",{name:"Quantize",exact:true});await expect(frame).toBeVisible();
    const rect=await frame.boundingBox();assert(rect.x>=7&&rect.y>=7&&rect.x+rect.width<=width-7&&rect.y+rect.height<=height-7,"quantize frame stays in viewport");
    assert(await frame.evaluate(el=>el.parentElement===document.body),"floating tool escapes clipped piano ancestors");
    const body=frame.locator(".floating-tool-content");assert(await body.evaluate(el=>el.scrollWidth<=el.clientWidth+1),"no horizontal content clipping");
    await frame.getByRole("button",{name:"Quantize whole part",exact:true}).scrollIntoViewIfNeeded();
    await expect(frame.getByRole("button",{name:"Quantize whole part",exact:true})).toBeVisible();
    bounds.push({width,height,rect});await page.screenshot({path:path.join(run,`quantize-window-${width}.png`)});
  }
  fs.writeFileSync(path.join(run,"floating-editor-bounds.json"),JSON.stringify({passed:true,viewportBounded:true,portaledToBody:true,bounds},null,2));
  results.push("quantize utility stays fully inside 1000x640 and 1480x900 viewports, escapes piano clipping, and keeps action controls reachable");
  fs.writeFileSync(path.join(run, "quantize-comparison.json"), JSON.stringify({
    reference: oracle.reference, observations: oracle.observation,
    referenceSnapshotSha256: oracle.snapshotSha256,
    equivalentFixture: fixture, ppq: 480, notesCompared: 96,
    noteOutputMatches: true, projectResetHistoryVerified: true,
    limitations: ["Equivalent MIDI is synthesized from previously observed original timings.", "No new SFM run, native CASM export or physical playback comparison is claimed."],
  }, null, 2));
}

export async function verifyExchangeWindowsNative({ ipc, run, results, data, persisted }) {
  const label = "parity-exchange-test";
  const recovery = path.join(data, `recovery-${label}.gss`);
  persisted.set(recovery, fs.existsSync(recovery) ? fs.readFileSync(recovery) : null);
  const fixture = path.join(run, "sfm-equivalent-480.mid");
  const main = await ipc("open_document", {path: fixture, docLabel:"main"});
  let secondary = await ipc("open_document", {path:fixture, docLabel:label});
  const section = main.sections[0].id;
  const secondSection = secondary.sections[0].id;
  const returned = path.join(run, "standard-exchange-return.mid");
  await ipc("export_phrase_midi", {path:returned, section, channel:9, revision:main.revision, docLabel:"main"});
  // Make the documents observably different before preparing their returns.
  secondary = await ipc("edit_document", {revision:secondary.revision, docLabel:label,
    command:{type:"updateNotes",section:secondSection,ids:secondary.notes.map(n=>n.id),deltaTick:0,deltaPitch:1}});
  assert.notEqual(secondary.notes[0].pitch, main.notes[0].pitch);
  const a = await ipc("preview_phrase_midi", {path:returned,section,channel:9,revision:main.revision,docLabel:"main"});
  const b = await ipc("preview_phrase_midi", {path:returned,section:secondSection,channel:9,revision:secondary.revision,docLabel:label});
  assert.notEqual(a.token,b.token);
  const appliedMain = await ipc("apply_phrase_midi", {token:a.token});
  assert.deepEqual(appliedMain.notes,main.notes);
  const appliedSecondary = await ipc("apply_phrase_midi", {token:b.token});
  const signature = v=>v.notes.map(n=>[n.start,n.duration,n.pitch,n.velocity]);
  assert.deepEqual(signature(appliedSecondary),signature(main));
  assert.equal(appliedSecondary.revision,secondary.revision+1);
  assert.deepEqual((await ipc("session_view",{docLabel:"main"})).notes,main.notes);
  const undone = await ipc("history",{redo:false,docLabel:label});
  assert.deepEqual(signature(undone),signature(secondary));
  results.push("independent project MIDI preview tokens survive concurrent previews and apply/Undo target only their own document");
}
