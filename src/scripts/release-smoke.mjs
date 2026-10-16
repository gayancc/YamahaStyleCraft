// Actual optimized executable with Vite stopped; no development module imports.
import {chromium} from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const run=path.resolve(`../docs/development/reports/release-${Date.now()}`);fs.mkdirSync(run,{recursive:true});
const browser=await chromium.connectOverCDP('http://127.0.0.1:9450');
try{
 const page=browser.contexts()[0].pages()[0];await page.waitForSelector('.brand');assert(!page.url().includes(':1420'));await page.screenshot({path:path.join(run,'packaged-launch.png')});
 const ipc=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
 const source=path.resolve('../docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY');let v=await ipc('open_document',{path:source});assert.equal(v.notes.length,825);const sec=v.sections.find(s=>s.label==='Main A'),note=v.notes.find(n=>n.channel===11&&n.start===sec.start);
 v=await ipc('edit_document',{revision:v.revision,command:{type:'updateNotes',section:sec.id,ids:[note.id],velocity:73}});assert.equal(v.notes.find(n=>n.id===note.id).velocity,73);
 const output=path.join(run,'release-edit.STY');await ipc('save_document',{path:output,projectFile:false});v=await ipc('open_document',{path:output});assert.equal(v.notes.find(n=>n.channel===11&&n.start===5760).velocity,73);
 assert.equal(v.notes.length,825);assert.equal(v.ppq,1920);assert.equal(v.numerator,3);
 fs.writeFileSync(path.join(run,'result.json'),JSON.stringify({result:'PACKAGED WINDOWS APP VERIFIED',url:page.url(),source,output,notes:v.notes.length,ppq:v.ppq,velocity:73,limitations:['Vite stopped; actual embedded assets and native IPC.','Installer install/uninstall not exercised.','No MIDI sent.']},null,2));fs.writeFileSync('../docs/development/reports/latest-release-run.txt',run);
 console.log(`PACKAGED WINDOWS APP VERIFIED: ${page.url()}, open/edit/export/reopen; Vite stopped`);
}finally{await browser.close();}
