// Verify the embedded release app, including the native WebView2 setting.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { chromium, expect } from "@playwright/test";

assert.equal(execFileSync("powershell",["-NoProfile","-Command","Get-Process -Name genos-style-studio -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id; exit 0"],{encoding:"utf8"}).trim(),"","Preserve a running user app");
const run=path.resolve(`../docs/piano-roll/validation/release-context-${Date.now()}`);
fs.mkdirSync(run,{recursive:true});
const data=path.join(process.env.APPDATA,"com.genosstylestudio.desktop");
fs.mkdirSync(data,{recursive:true});
const backup=new Map();
for(const name of ["recovery.gss","prefs.json","recent-files.json"]){
  const file=path.join(data,name);backup.set(file,fs.existsSync(file)?fs.readFileSync(file):null);
}
const style=fs.readFileSync("../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY");
let child,browser;
try {
  fs.writeFileSync(path.join(data,"recovery.gss"),JSON.stringify({schema:"genos.style-project.v1",name:"Context menu check",sha256:crypto.createHash("sha256").update(style).digest("hex"),style:[...style]}));
  child=spawn(path.resolve("target/release/genos-style-studio.exe"),[],{windowsHide:true,stdio:"ignore",env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(run,"profile"),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:"--remote-debugging-port=9451"}});
  await expect.poll(async()=>{try{return (await fetch("http://127.0.0.1:9451/json/version")).ok;}catch{return false;}},{timeout:30000}).toBe(true);
  browser=await chromium.connectOverCDP("http://127.0.0.1:9451");
  const page=browser.contexts()[0].pages()[0];
  await page.waitForSelector(".brand",{state:"attached"});
  assert(!page.url().includes(":1420"),"Release must use embedded assets");
  const native=target=>target.evaluate(()=>window.__TAURI_INTERNALS__.invoke("native_context_menu_enabled"));
  assert.equal(await native(page),false);
  await page.getByRole("button",{name:"Restore session",exact:true}).click();
  await expect(page.getByRole("button",{name:"Restore session",exact:true})).toHaveCount(0);
  await expect(page.getByRole("slider",{name:"Selected part pan",exact:true})).toBeVisible();
  assert.equal(await page.locator(".transport .midi-owner-btn").count(),0);
  await page.screenshot({path:path.join(run,"release-studio-workspace.png")});
  await page.getByRole("button",{name:"Mixer",exact:true}).click();
  await expect(page.getByRole("slider",{name:"Ch 11 volume",exact:true})).toBeVisible();
  await expect(page.getByRole("slider",{name:"Ch 11 pan",exact:true})).toBeVisible();
  await page.screenshot({path:path.join(run,"release-studio-mixer.png")});
  await page.getByRole("button",{name:"Piano roll",exact:true}).click();
  await page.getByRole("button",{name:"Select notes",exact:true}).click();
  const releaseView=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke("session_view",{docLabel:"main"}));
  const releaseSectionLabel=await page.locator(".section-chip.selected > span").innerText();
  const releaseSection=releaseView.sections.find(s=>s.label===releaseSectionLabel);
  const releaseChannel=Number(await page.locator(".track.selected .channel-number").innerText());
  const releaseNote=releaseView.notes.find(n=>n.channel===releaseChannel&&n.start>=releaseSection.start&&n.start<releaseSection.end);assert(releaseNote);
  const releaseZoom=Number((await page.locator(".zoom-label").innerText()).replace("%",""))/100;
  const releaseScale=72*releaseZoom/releaseView.ppq;
  await page.locator(".piano-scroll").evaluate((el,{note,section,scale})=>{el.scrollTop=(127-note.pitch)*18-100;el.scrollLeft=Math.max(0,60+(note.start-section.start)*scale-150);},{note:releaseNote,section:releaseSection,scale:releaseScale});
  const releasePoint=await page.locator(".piano-scroll").evaluate((el,{note,section,scale})=>{const r=el.querySelector("canvas").getBoundingClientRect();return {x:r.x+60+(note.start-section.start)*scale-el.scrollLeft+Math.min(8,note.duration*scale/2),y:r.y+28+(127-note.pitch)*18-el.scrollTop+9};},{note:releaseNote,section:releaseSection,scale:releaseScale});
  await page.mouse.dblclick(releasePoint.x,releasePoint.y);
  const utility=page.getByRole("dialog",{name:"Note properties",exact:true});
  await expect(utility).toBeVisible();assert.equal(await utility.getAttribute("aria-modal"),"false");assert.equal(await page.locator(".modal-backdrop").count(),0);
  await page.screenshot({path:path.join(run,"release-note-utility.png")});
  await utility.getByRole("button",{name:"Cancel",exact:true}).click();
  const afterUtility=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke("session_view",{docLabel:"main"}));assert.equal(afterUtility.revision,releaseView.revision);
  await page.getByRole("button",{name:"Piano tools",exact:true}).click();
  await page.getByLabel("Note transform",{exact:true}).selectOption("quantize");
  await page.setViewportSize({width:1000,height:640});
  const releaseQuantize=page.getByRole("dialog",{name:"Quantize",exact:true});await expect(releaseQuantize).toBeVisible();
  await expect.poll(async()=>{const r=await releaseQuantize.boundingBox();return r.x>=7&&r.y>=7&&r.x+r.width<=993&&r.y+r.height<=633;}).toBe(true);
  assert(await releaseQuantize.evaluate(el=>el.parentElement===document.body));
  await releaseQuantize.getByRole("button",{name:/^Quantize (whole part|selection)$/}).scrollIntoViewIfNeeded();
  await page.screenshot({path:path.join(run,"release-quantize-window.png")});
  await releaseQuantize.getByRole("button",{name:"Close Quantize",exact:true}).click();
  await page.setViewportSize({width:1480,height:900});
  await page.locator(".output-select").click();
  const devices=page.getByRole("dialog",{name:"MIDI connections",exact:true});
  await expect(devices).toBeVisible();
  const deviceBounds=await devices.boundingBox();
  assert(deviceBounds.width<=460&&deviceBounds.height<=540);
  assert.equal(await devices.getAttribute("aria-modal"),"false");
  for(const name of ["Start monitor","Start capture","Refresh devices","Done"]) await expect(devices.getByRole("button",{name,exact:true})).toBeVisible();
  await page.screenshot({path:path.join(run,"release-midi-connections.png")});
  await devices.getByRole("button",{name:"Recording & MIDI settings…",exact:true}).click();
  await expect(devices).toBeHidden();
  await page.getByRole("dialog",{name:"Recording & MIDI",exact:true}).getByRole("button",{name:"Close",exact:true}).click();
  await expect(devices).toBeVisible();
  await devices.getByRole("button",{name:"Done",exact:true}).click();
  await page.getByRole("button",{name:"Detach Piano Roll (separate window)",exact:true}).click();
  let piano;
  await expect.poll(()=>{
    piano=browser.contexts().flatMap(c=>c.pages()).find(p=>new URL(p.url()).searchParams.get("detach")==="piano");return !!piano;
  }).toBe(true);
  await piano.getByRole("img",{name:/^Piano roll\./}).waitFor();
  assert.equal(await native(piano),false);
  // Check real right-click routing in the release, in the detached editor.
  const box=await piano.locator(".piano-scroll canvas").boundingBox();
  await piano.mouse.click(box.x+75,box.y+70,{button:"right"});
  await expect(piano.getByRole("menu",{name:"Piano actions"})).toBeVisible();
  await piano.screenshot({path:path.join(run,"release-piano-context-menu.png")});
  fs.writeFileSync(path.join(run,"result.json"),JSON.stringify({passed:true,embeddedUrl:page.url(),mainNativeMenusEnabled:false,detachedPianoNativeMenusEnabled:false,pianoActionsMenuVisible:true,hardwareMidiOpened:false,functionLedWorkspaceVisible:true,mixerFadersAndKnobsVisible:true,desktopNoteUtilityVerified:true,quantizeWindowUnclipped:true,compactMidiUtilityVerified:true,deviceBounds,recordingSettingsReturnVerified:true},null,2));
  console.log(`RELEASE CONTEXT MENU VERIFIED: ${run}`);
}finally{
  if(browser)await browser.close();
  if(child&&child.exitCode===null){const ended=new Promise(resolve=>child.once("exit",resolve));child.kill();await ended;}
  for(const [file,bytes]of backup){if(bytes!==null)fs.writeFileSync(file,bytes);else if(fs.existsSync(file))fs.unlinkSync(file);}
}
