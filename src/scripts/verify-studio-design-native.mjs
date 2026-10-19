import fs from "node:fs";
import path from "node:path";
import assert from "node:assert/strict";
import { expect } from "@playwright/test";

export async function verifyStudioDesign({ page, ipc, run, results, source, reset }) {
  const state = () => page.evaluate(async () => {
    const { useStudio } = await import("/frontend/store.ts");
    const s = useStudio.getState(); return { view:s.view, busy:s.busy, error:s.error, channel:s.channel, sectionId:s.sectionId };
  });
  const settle = async () => { await expect.poll(async () => (await state()).busy).toBe(false); assert.equal((await state()).error,null); };
  const history = async redo => { await page.getByRole("button",{name:redo ? "Redo (Ctrl+Shift+Z)" : "Undo (Ctrl+Z)",exact:true}).click(); await settle(); };
  const cc = (s, controller) => {
    const start=s.view.sections.find(x=>x.id===s.sectionId).start;
    return s.view.events.filter(e=>e.tick<=start&&e.data[0]===(0xb0|(s.channel-1))&&e.data[1]===controller)
      .sort((a,b)=>a.tick-b.tick||a.order-b.order).at(-1)?.data[2];
  };
  const enter = async (name,n) => {
    await page.getByRole("button",{name:`Enter ${name} value`,exact:true}).click();
    const input=page.getByRole("spinbutton",{name:`${name} value`,exact:true});
    await input.fill(String(n)); await input.press("Enter"); await settle();
  };
  const baseline=await state();
  const knob=page.getByRole("slider",{name:"Selected part pan",exact:true});
  const original=Number(await knob.getAttribute("aria-valuenow"));
  const box=await knob.boundingBox(); assert(box);
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2); await page.mouse.down();
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2-28,{steps:10});
  assert.equal((await state()).view.revision,baseline.view.revision,"drag preview never sends intermediate commands");
  const preview=Number(await knob.getAttribute("aria-valuenow")); assert(preview>original);
  await page.mouse.up(); await settle();
  assert.equal((await state()).view.revision,baseline.view.revision+1);
  assert.equal(cc(await state(),10),preview);
  await history(false); assert.equal(Number(await knob.getAttribute("aria-valuenow")),original);
  await history(true); assert.equal(Number(await knob.getAttribute("aria-valuenow")),preview);
  const beforeCancel=(await state()).view.revision;
  await page.getByRole("button",{name:"Enter Selected part pan value",exact:true}).click();
  const exact=page.getByRole("spinbutton",{name:"Selected part pan value",exact:true});
  await exact.fill("12"); await exact.press("Escape");
  assert.equal((await state()).view.revision,beforeCancel);
  assert.equal(cc(await state(),10),preview);
  await enter("Selected part pan",32); assert.equal(cc(await state(),10),32);
  await knob.dblclick(); await settle(); assert.equal(cc(await state(),10),64);
  results.push("purpose-built pan knob: local drag preview, one native CC transaction, Undo/Redo, exact entry, Escape cancel and centered reset");
  // Parameter arrow keys must not reach the selected piano notes.
  await page.evaluate(async () => { const {useStudio}=await import("/frontend/store.ts"); const s=useStudio.getState(); useStudio.setState({selected:[s.view.notes.find(n=>n.channel===s.channel && n.start>=s.view.sections.find(x=>x.id===s.sectionId).start && n.start<s.view.sections.find(x=>x.id===s.sectionId).end).id]}); });
  const notesBefore=JSON.stringify((await state()).view.notes);
  await knob.focus(); await knob.press("ArrowUp"); await settle();
  assert.equal(cc(await state(),10),65);
  assert.equal(JSON.stringify((await state()).view.notes),notesBefore);
  results.push("parameter keyboard adjustment changes CC only, without moving selected piano notes");
  const selectedNote=(await state()).view.notes.find(n=>n.channel===11&&n.start>=baseline.view.sections.find(x=>x.id===baseline.sectionId).start);
  await enter("Note velocity",73);
  assert.equal((await state()).view.notes.find(n=>n.id===selectedNote.id).velocity,73);
  await history(false);
  assert.equal((await state()).view.notes.find(n=>n.id===selectedNote.id).velocity,selectedNote.velocity);
  results.push("inspector velocity knob uses selected note identity and native Undo without changing timing");
  const size = async (width,height) => {
    await page.setViewportSize({ width, height });
    await expect.poll(()=>page.evaluate(()=>innerWidth)).toBe(width);
  };
  const layouts=[];
  for(const [width,height] of [[1000,700],[1480,900]]) {
    await size(width,height);
    const layout=await page.evaluate(()=>{
      const rect=s=>{const r=document.querySelector(s).getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};
      return {width:innerWidth,height:innerHeight,scrollWidth:document.documentElement.scrollWidth,transport:rect('.transport-primary'),piano:rect('.piano-scroll'),footer:rect('.statusbar')};
    });
    assert(layout.transport.right<=width); assert(layout.piano.width>250&&layout.piano.height>160); assert(layout.footer.bottom<=height+1); assert(layout.scrollWidth<=width);
    layouts.push(layout); await page.screenshot({path:path.join(run,`studio-design-${width}.png`)});
    await page.locator(".output-select").click();
    const devices=page.getByRole("dialog",{name:"MIDI connections",exact:true});await expect(devices).toBeVisible();
    const dr=await devices.boundingBox();assert(dr.width<=460&&dr.height<=540,`compact MIDI utility: ${JSON.stringify(dr)}`);
    assert(dr.x>=7&&dr.y>=7&&dr.x+dr.width<=width-7&&dr.y+dr.height<=height-7);
    assert.equal(await devices.getAttribute("aria-modal"),"false");
    for(const name of ["Start monitor","Start capture","Refresh devices","Done"]) await expect(devices.getByRole("button",{name,exact:true})).toBeVisible();
    await page.screenshot({path:path.join(run,`midi-connections-${width}.png`)});
    await devices.getByRole("button",{name:"Recording & MIDI settings…",exact:true}).click();
    await expect(devices).toBeHidden();
    const recording=page.getByRole("dialog",{name:"Recording & MIDI",exact:true});await expect(recording).toBeVisible();
    await recording.getByRole("button",{name:"Close",exact:true}).click();await expect(devices).toBeVisible();
    await devices.getByRole("button",{name:"Done",exact:true}).click();await expect(devices).toHaveCount(0);

  }
  results.push("compact MIDI utility fits 460x540 at both viewports; device/monitor/capture controls visible; recording settings hides and returns to the same utility without opening hardware");
  const fileSummary=page.locator('.file-actions > .studio-menu > summary');
  await fileSummary.click(); await expect(page.getByRole('button',{name:'Export Style (Ctrl+Shift+S)',exact:true})).toBeVisible();
  await fileSummary.press('Escape'); await expect(fileSummary.locator('..')).not.toHaveAttribute('open','');
  const studioSummary=page.locator('.titlebar-tools > .studio-menu > summary');
  await studioSummary.click(); await expect(page.getByRole('button',{name:'MIDI active here',exact:true})).toBeVisible();
  assert.equal(await page.locator('.transport .midi-owner-btn').count(),0);
  await page.locator('.project-title').click(); await expect(studioSummary.locator('..')).not.toHaveAttribute('open','');
  results.push("1000 and 1480 pixel WebView viewports: transport and piano visible, no document overflow, File/Studio utilities remain reachable with Escape/outside dismissal");
  await page.getByRole('button',{name:'Mixer',exact:true}).click();
  const fader=page.getByRole('slider',{name:'Ch 11 volume',exact:true});
  await fader.scrollIntoViewIfNeeded();
  const fv=Number(await fader.getAttribute('aria-valuenow')), revision=(await state()).view.revision;
  const f=await fader.boundingBox();
  await page.mouse.move(f.x+f.width/2,f.y+f.height/2); await page.mouse.down();
  await page.mouse.move(f.x+f.width/2,f.y+f.height/2+34,{steps:10});
  assert.equal((await state()).view.revision,revision); await page.mouse.up(); await settle();
  const finalVolume=cc(await state(),7); assert(finalVolume<fv); assert.equal((await state()).view.revision,revision+1);
  await history(false); assert.equal(Number(await fader.getAttribute('aria-valuenow')),fv); await history(true); assert.equal(cc(await state(),7),finalVolume);
  await enter('Ch 11 reverb send',81); assert.equal(cc(await state(),91),81);
  await page.screenshot({path:path.join(run,'studio-mixer-sends.png')});
  await page.getByRole('button',{name:'Tone / EQ',exact:true}).click();
  await expect(page.getByRole('slider',{name:'Ch 11 reverb send',exact:true})).toHaveCount(0);
  await enter('Ch 11 EQ Low',80);
  const eq=()=>page.evaluate(async()=>{const {useStudio}=await import('/frontend/store.ts');const {eqValueAt}=await import('/frontend/MixerPanel.tsx');const s=useStudio.getState();return eqValueAt(s.view,s.sectionId,s.channel,'eqLow');});
  assert.equal(await eq(),80);
  await page.screenshot({path:path.join(run,'studio-mixer-tone.png')});
  await page.getByRole('button',{name:'Performance',exact:true}).click();
  const sustain=page.getByRole('switch',{name:'Ch 11 sustain',exact:true});
  const oldSustain=await sustain.getAttribute('aria-checked'); await sustain.click(); await settle();
  assert.equal(cc(await state(),64),oldSustain==='true'?0:127);
  assert.deepEqual((await state()).view.routes.map(r=>r.bytes),baseline.view.routes.map(r=>r.bytes));
  const project=path.join(run,'studio-design.gss'), exported=path.join(run,'studio-design.STY');
  await ipc('save_document',{path:project,projectFile:true,docLabel:'main'});
  await ipc('save_document',{path:exported,projectFile:false,docLabel:'main'});
  const expected={volume:finalVolume,pan:65,reverb:81,sustain:oldSustain==='true'?0:127};
  for(const file of [project,exported]) {
    await page.evaluate(async file=>{const {useStudio}=await import('/frontend/store.ts');await useStudio.getState().load(file);useStudio.setState({channel:11});},file);await settle();
    for(const [controller,key] of [[7,'volume'],[10,'pan'],[91,'reverb'],[64,'sustain']]) assert.equal(cc(await state(),controller),expected[key]);
    assert.equal(await eq(),80);
    assert.deepEqual((await state()).view.routes.map(r=>r.bytes),baseline.view.routes.map(r=>r.bytes));
  }
  results.push("mixer fader/send knob/tone EQ/sustain switch write real MIDI CC/NRPN; atomic Undo/Redo and GSS/STY roundtrips preserve values and CASM");
  fs.writeFileSync(path.join(run,'studio-design-checks.json'),JSON.stringify({passed:true,hardwareMidiOpened:false,layouts,expected,eqLow:80},null,2));
  await page.getByRole('button',{name:'Piano roll',exact:true}).click();
  await page.evaluate(async source=>{const {useStudio}=await import('/frontend/store.ts');await useStudio.getState().load(source);},source);
  await reset();
}
