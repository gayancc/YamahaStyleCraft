// Embedded-release check: stock voices work without any imported catalog or PDF.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { chromium, expect } from "@playwright/test";

assert.equal(execFileSync("powershell", ["-NoProfile", "-Command", "Get-Process -Name genos-style-studio -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id; exit 0"], { encoding: "utf8" }).trim(), "", "Preserve a running user app");
const run = path.resolve(`../docs/piano-roll/validation/factory-voices-${Date.now()}`);
fs.mkdirSync(run, { recursive: true });
// Run a standalone copy with no adjacent source tree, data files or PDF.
const executable = path.join(run, "standalone", "genos-style-studio.exe");
fs.mkdirSync(path.dirname(executable));
fs.copyFileSync(path.resolve("target/release/genos-style-studio.exe"), executable);
const data = path.join(process.env.APPDATA, "com.genosstylestudio.desktop");
fs.mkdirSync(data, { recursive: true });
const backup = new Map();
for (const name of ["recovery.gss", "prefs.json", "recent-files.json", "voice-catalog.json"]) {
  const file = path.join(data, name);
  backup.set(file, fs.existsSync(file) ? fs.readFileSync(file) : null);
}
const catalogPath = path.join(data, "voice-catalog.json");
const style = fs.readFileSync("../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
const checks = [];
const wireChecks = [];
let captureGuard = { status: 'BLOCKED', reason: 'Not checked yet' }, hardwareInputOpened = false;
let child, browser, page, launches = 0;
const ipc = (cmd, args = {}) => page.evaluate(({ cmd, args }) => window.__TAURI_INTERNALS__.invoke(cmd, args), { cmd, args });
// Independent SMF decoder: verify the wire address, rather than asking the
// application that authored it to resolve its own output.
function wireAddress(file, at, channel) {
  const bytes=fs.readFileSync(file), events=[]; let start=0;
  while(start<bytes.length) {
    const tag=bytes.toString('ascii',start,start+4), length=bytes.readUInt32BE(start+4), end=start+8+length;
    assert(end<=bytes.length);
    if(tag==='MTrk') {
      let cursor=start+8,tick=0,running=0;
      const vlq=()=>{let value=0;for(let i=0;i<4;i++){assert(cursor<end);const b=bytes[cursor++];value=value*128+(b&127);if(!(b&128))return value;}throw new Error('Bad VLQ');};
      while(cursor<end) {
        tick+=vlq();let status=bytes[cursor]&128?bytes[cursor++]:running;assert(status);
        if(status===255){running=0;cursor++;const n=vlq();cursor+=n;}
        else if(status===240||status===247){running=0;const n=vlq();cursor+=n;}
        else {running=status;const count=[12,13].includes(status>>4)?1:2;const data=[...bytes.subarray(cursor,cursor+count)];cursor+=count;events.push({tick,status,data,order:events.length});}
      }
    }
    start=end;
  }
  let msb=0,lsb=0,address=null;
  for(const event of events.filter(e=>e.tick<=at&&(e.status&15)===channel-1).sort((a,b)=>a.tick-b.tick||a.order-b.order)) {
    if(event.status>>4===11&&event.data[0]===0)msb=event.data[1];
    if(event.status>>4===11&&event.data[0]===32)lsb=event.data[1];
    if(event.status>>4===12)address={msb,lsb,program:event.data[0]};
  }
  return address;
}
async function launch(profile) {
  const endpoint = `http://127.0.0.1:${9452 + launches++}`;
  const port = new URL(endpoint).port;
  child = spawn(executable, [], { cwd: path.dirname(executable), windowsHide: true, stdio: "ignore", env: { ...process.env, WEBVIEW2_USER_DATA_FOLDER: path.join(run, profile), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` } });
  await expect.poll(async () => { try { return (await fetch(`${endpoint}/json/version`)).ok; } catch { return false; } }, { timeout: 30000 }).toBe(true);
  browser = await chromium.connectOverCDP(endpoint);
  page = browser.contexts()[0].pages()[0];
  await page.waitForSelector(".brand", { state: "attached" });
  assert(!page.url().includes(":1420"), "Use embedded release assets");
  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__voiceTestCalls = [];
    window.__TAURI_INTERNALS__.invoke = (cmd, args, ...rest) => {
      window.__voiceTestCalls.push(cmd);
      return invoke(cmd, args, ...rest);
    };
  });
}
async function close() {
  if (browser) { await browser.close(); browser = null; }
  if (child && child.exitCode === null) {
    const ended = new Promise(resolve => child.once("exit", resolve)); child.kill(); await ended;
  }
  child = null;
}
try {
  for (const file of backup.keys()) if (fs.existsSync(file)) fs.unlinkSync(file);
  fs.writeFileSync(path.join(data, "recovery.gss"), JSON.stringify({ schema: "genos.style-project.v1", name: "Factory voices", sha256: crypto.createHash("sha256").update(style).digest("hex"), style: [...style] }));
  await launch("clean-profile");
  const status = await ipc("voice_catalog_status"); assert.equal(status.count, 2456);
  assert(!fs.existsSync(catalogPath));
  const voices = await ipc("search_voice_catalog", { query: "CFX ConcertGrand" });
  const voice = voices.voices.find(v => v.name === "CFX ConcertGrand"); assert(voice);
  assert((await ipc('search_voice_catalog',{query:'104:21:1'})).voices.some(v=>v.name==='CFX ConcertGrand'));
  const kits = await ipc("search_drum_kits", { query: "" }); assert(kits.total > 0);
  assert.equal(kits.total,58);
  assert(kits.voices.every(v => v.category === "DrumKit" || v.msb === 120));
  const addresses=new Set();
  for(let offset=0;offset<status.count;offset+=40) {
    const result=await ipc('search_voice_catalog',{query:'',kind:'all',offset,limit:40});
    assert.equal(result.total,2456);assert.equal(result.offset,offset);
    for(const v of result.voices){const key=`${v.msb}:${v.lsb}:${v.program}`;assert(!addresses.has(key));addresses.add(key);}
  }
  assert.equal(addresses.size,2456);
  for(const [kind,total] of [['voices',2398],['panel',1711],['xg',480],['gm2',265],['drums',58]])
    assert.equal((await ipc('search_voice_catalog',{query:'',kind,limit:40})).total,total);
  checks.push("Standalone embedded executable: all 2,456 real Genos-compatible voices are reachable across pages; 58 kits; no PDF or cache");
  await page.getByRole("button", { name: "Restore session", exact: true }).click();
  await page.locator(".section-chip").filter({ hasText: /Main A/ }).click();
  await page.locator(".track .channel-number").filter({ hasText: /^11$/ }).click();
  await page.locator(".part-voice").click();
  const picker = page.getByRole("dialog", { name: "Genos voices", exact: true });
  await expect(picker.getByLabel("Search voices", { exact: true })).toBeVisible();
  await expect(picker.getByRole("status", { name: "Voice catalog", exact: true })).toContainText("2,456 factory voices");
  assert.equal(await picker.getAttribute('aria-modal'),'false');
  assert(await picker.evaluate(el=>el.parentElement===document.body));
  await expect(picker.getByRole("button", { name: "Import Data List PDF…", exact: true })).toBeHidden();
  await expect(picker.getByRole('button',{name:'Next voices',exact:true})).toBeEnabled();
  await picker.getByRole('button',{name:'Next voices',exact:true}).click();
  await expect(picker.getByRole('status').filter({hasText:'41–80 of 2456 voices'})).toBeVisible();
  await picker.getByRole('button',{name:'Previous voices',exact:true}).click();
  await expect(picker.getByRole('status').filter({hasText:'1–40 of 2456 voices'})).toBeVisible();
  await page.setViewportSize({width:1000,height:700});
  await expect.poll(async()=>{const r=await picker.boundingBox();return r.x>=7&&r.y>=7&&r.x+r.width<=993&&r.y+r.height<=693;}).toBe(true);
  await page.screenshot({path:path.join(run,'voice-browser-1000.png')});
  await page.setViewportSize({width:1480,height:900});
  await picker.getByLabel("Search voices", { exact: true }).fill(voice.name);
  await picker.getByRole("button", { name: new RegExp(`^${voice.name}`) }).click();
  await page.screenshot({ path: path.join(run, "factory-voice-browser.png") });
  const before = await ipc("session_view", { docLabel: "main" });
  const section = before.sections.find(s => s.label === "Main A"); assert(section);
  await picker.getByRole("button", { name: "Apply voice", exact: true }).click();
  await expect(picker).toHaveCount(0);
  const applied = await ipc("session_view", { docLabel: "main" }); assert.equal(applied.revision, before.revision + 1);
  const args = view => ({ section: section.id, channel: 11, revision: view.revision });
  assert.equal((await ipc("resolve_catalog_voice", args(applied))).name, voice.name);
  assert.deepEqual(applied.notes, before.notes);
  assert.deepEqual(applied.routes.map(r => r.bytes), before.routes.map(r => r.bytes));
  await page.getByRole("button", { name: "Undo (Ctrl+Z)", exact: true }).click();
  await expect.poll(async () => (await ipc("session_view", { docLabel: "main" })).revision).toBe(applied.revision + 1);
  const undone = await ipc("session_view", { docLabel: "main" }); assert.deepEqual(undone.events, before.events);
  await page.getByRole("button", { name: "Redo (Ctrl+Shift+Z)", exact: true }).click();
  await expect.poll(async () => (await ipc("session_view", { docLabel: "main" })).revision).toBe(undone.revision + 1);
  const redone = await ipc("session_view", { docLabel: "main" }); assert.deepEqual(redone.events, applied.events);
  checks.push("Voice picker opens directly; optional import stays collapsed; factory voice apply is one transaction with exact Undo/Redo, preserved notes and CASM");
  const panelFile=path.join(run,'panel-voice.STY');
  await ipc('save_document',{path:panelFile,projectFile:false,docLabel:'main'});
  assert.deepEqual(wireAddress(panelFile,section.start,11),{msb:104,lsb:21,program:0});
  wireChecks.push({group:'panel',file:panelFile,address:wireAddress(panelFile,section.start,11)});
  const compatibilityExamples=[
    {kind:'xg',query:'GrandPiano',address:{msb:0,lsb:0,program:0}},
    {kind:'gm2',query:'Gunshot',address:{msb:121,lsb:0,program:127}}
  ];
  for(const sample of compatibilityExamples) {
    const result=await ipc('search_voice_catalog',{query:sample.query,kind:sample.kind,offset:0,limit:100});
    const item=result.voices.find(v=>v.msb===sample.address.msb&&v.lsb===sample.address.lsb&&v.program===sample.address.program);assert(item);
    await page.locator('.part-voice').click();
    await picker.getByLabel('Voice bank group',{exact:true}).selectOption(sample.kind);
    await picker.getByLabel('Search voices',{exact:true}).fill(item.name);
    await picker.locator('.voice-results > button').filter({has:page.getByText(item.name,{exact:true})}).click();
    await expect(picker.getByLabel('Voice selection preview')).toContainText(`Program ${item.program+1}`);
    await page.screenshot({path:path.join(run,`${sample.kind}-voice-browser.png`)});
    await picker.getByRole('button',{name:'Apply voice',exact:true}).click();await expect(picker).toHaveCount(0);
    const view=await ipc('session_view',{docLabel:'main'});
    assert.equal((await ipc('resolve_catalog_voice',{section:section.id,channel:11,revision:view.revision})).name,item.name);
    const file=path.join(run,`${sample.kind}-voice.STY`);await ipc('save_document',{path:file,projectFile:false,docLabel:'main'});
    assert.deepEqual(wireAddress(file,section.start,11),sample.address);
    wireChecks.push({group:sample.kind,file,address:wireAddress(file,section.start,11)});
  }
  checks.push('Panel, XG/GM and GM2 selections produce independently decoded CC0/CC32/Program Change bytes, including PC 1 to raw 0 and PC 128 to raw 127');
  const finalVoice='Gunshot (gm2)';
  await page.locator(".track .channel-number").filter({ hasText: /^0?9$/ }).click();
  await page.getByRole("button", { name: "Drum kits (Ch 9–10)", exact: true }).click();
  const drums = page.getByRole("dialog", { name: "Drum kits", exact: true });
  await expect(drums.getByRole("option").first()).toBeVisible();
  const kit = kits.voices[0];
  await drums.getByLabel("Search drum kits", { exact: true }).fill(kit.name);
  await drums.getByRole("option").filter({ hasText: kit.name }).click();
  await page.screenshot({ path: path.join(run, "factory-drum-kits.png") });
  const beforeKit = await ipc("session_view", { docLabel: "main" });
  await drums.getByRole("button", { name: /Apply kit to Ch/ }).click();
  await expect(drums).toHaveCount(0);
  const afterKit = await ipc("session_view", { docLabel: "main" }); assert.equal(afterKit.revision, beforeKit.revision + 1);
  assert.equal((await ipc("resolve_catalog_voice", { section: section.id, channel: 9, revision: afterKit.revision })).name, kit.name);
  const owners = await ipc("drum_channel_status", { section: section.id, revision: afterKit.revision });
  assert.equal(owners.find(o => o.channel === 9).voice.name, kit.name);
  checks.push("Built-in drum-kit picker applies a verified kit on channel 9 and resolves the section-start kit without import");
  const expectedNotes = afterKit.notes;
  for (const [filename, projectFile] of [["factory-voices.gss", true], ["factory-voices.STY", false]]) {
    const file = path.join(run, filename);
    await ipc("save_document", { path: file, projectFile, docLabel: "main" });
    const reloaded = await ipc("open_document", { path: file, docLabel: "main" });
    const loadedSection = reloaded.sections.find(s => s.label === "Main A");
    for (const [channel, expected] of [[11, finalVoice], [9, kit.name]])
      assert.equal((await ipc("resolve_catalog_voice", { section: loadedSection.id, channel, revision: reloaded.revision })).name, expected);
    assert.deepEqual(reloaded.notes.map(n => [n.channel, n.pitch, n.start, n.duration, n.velocity]), expectedNotes.map(n => [n.channel, n.pitch, n.start, n.duration, n.velocity]));
    assert.deepEqual(reloaded.routes.map(r => r.bytes), before.routes.map(r => r.bytes));
  }
  checks.push("GSS save and STY export reopen with the selected factory voice and kit, preserving musical notes and CASM");
  const beforeExpansion=await ipc('session_view',{docLabel:'main'});
  const expansionSection=beforeExpansion.sections.find(s=>s.label==='Main A');
  const recoveredAddress={msb:63,lsb:1,program:12};
  const afterExpansion=await ipc('edit_document',{revision:beforeExpansion.revision,docLabel:'main',command:{type:'setVoice',section:expansionSection.id,channel:11,address:recoveredAddress}});
  assert.equal(afterExpansion.revision,beforeExpansion.revision+1);
  const expansionFile=path.join(run,'recovered-expansion-address.STY');
  await ipc('save_document',{path:expansionFile,projectFile:false,docLabel:'main'});
  assert.deepEqual(wireAddress(expansionFile,expansionSection.start,11),recoveredAddress);
  wireChecks.push({group:'recovered expansion',file:expansionFile,address:wireAddress(expansionFile,expansionSection.start,11)});
  assert.deepEqual(afterExpansion.notes,beforeExpansion.notes);
  assert.deepEqual(afterExpansion.routes.map(r=>r.bytes),beforeExpansion.routes.map(r=>r.bytes));
  checks.push('Recovered expansion-address edit uses the real native SetVoice transaction and writes independently decoded raw bank/program bytes without factory-name substitution');
  const devices=await ipc('scan_devices');
  if(!devices.inputs.length) captureGuard={status:'BLOCKED',reason:'The current native MIDI device scan reports zero input ports'};
  else {
    let recording=false;
    try {
      try {
        await ipc('start_capture',{name:devices.inputs[0].name,thruIndex:null,thruName:null,docLabel:'main'});
        hardwareInputOpened=true;recording=true;
      } catch(error) { captureGuard={status:'BLOCKED',reason:`MIDI input could not be opened: ${String(error)}`}; }
      if(recording) {
        assert.equal((await ipc('capture_snapshot')).recording,true);
        const baseline=await ipc('session_view',{docLabel:'main'});
        await assert.rejects(ipc('apply_catalog_voice',{section:expansionSection.id,channel:11,revision:baseline.revision,address:{msb:104,lsb:21,program:0}}),/Stop playback and capture before applying a voice/);
        await assert.rejects(ipc('edit_document',{revision:baseline.revision,docLabel:'main',command:{type:'setVoice',section:expansionSection.id,channel:11,address:recoveredAddress}}),/Stop capture before applying a voice/);
        const unchanged=await ipc('session_view',{docLabel:'main'});
        assert.equal(unchanged.revision,baseline.revision);assert.deepEqual(unchanged.events,baseline.events);
        captureGuard={status:'VERIFIED',reason:'Active input-only capture rejects both factory and recovered voice edits without changing revision/events; MIDI thru disabled'};
        checks.push('Active input-only capture blocks both factory and recovered SetVoice edits atomically; no MIDI output or audition');
      }
    } finally { if(recording) {await ipc('stop_capture');await ipc('clear_capture');} }
  }
  assert(!fs.existsSync(catalogPath), "Factory browsing does not create an imported catalog");
  const calls = await page.evaluate(() => window.__voiceTestCalls);
  assert(!calls.includes("import_voice_catalog"));
  assert(!calls.some(c => ["play", "preview_drum_note", "start_monitor"].includes(c)));
  await close();
  await launch("restart-profile");
  assert.equal((await ipc("voice_catalog_status")).count, 2456);
  assert.equal((await ipc("search_voice_catalog", { query: voice.name })).voices[0].name, voice.name);
  assert(!fs.existsSync(catalogPath));
  checks.push("Application restart retains immediate factory voice availability without a local catalog");
  await close();
  const corrupt = Buffer.from("{damaged previous import"); fs.writeFileSync(catalogPath, corrupt);
  await launch("damaged-cache-profile");
  assert.equal((await ipc("voice_catalog_status")).count, 2456);
  assert.equal((await ipc("search_voice_catalog", { query: voice.name })).voices[0].name, voice.name);
  assert((await ipc("search_drum_kits", { query: "" })).total > 0);
  assert.deepEqual(fs.readFileSync(catalogPath), corrupt);
  checks.push("Damaged legacy catalog falls back to embedded stock voices and kits without overwriting the damaged file");
  fs.writeFileSync(path.join(run, "result.json"), JSON.stringify({ passed: true, count: status.count, drumKitCount: kits.total, hardwareMidiOpened: hardwareInputOpened, hardwareInputOpened, hardwareOutputOpened:false, captureGuard, pdfRequired: false, importedCatalogRequired: false, wireChecks, checks }, null, 2));
  console.log(`FACTORY VOICES RELEASE VERIFIED: ${run}\n${checks.length} checks`);
} catch (error) {
  if (page) await page.screenshot({ path: path.join(run, "failure.png") }).catch(() => {});
  fs.writeFileSync(path.join(run, "result.json"), JSON.stringify({ passed: false, error: String(error), checks }, null, 2));
  throw error;
} finally {
  await close();
  for (const [file, bytes] of backup) {
    if (bytes !== null) fs.writeFileSync(file, bytes);
    else if (fs.existsSync(file)) fs.unlinkSync(file);
  }
}
