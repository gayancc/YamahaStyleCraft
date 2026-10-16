// Isolated Windows installation test. Refuses an existing named installation.
// Installs only into a fresh directory under docs/development/reports and removes
// only that installation with its own generated uninstaller.
import fs from 'node:fs';
import path from 'node:path';
import {spawn} from 'node:child_process';
import assert from 'node:assert/strict';
const root=fs.realpathSync('.'),reports=path.resolve('../docs/development/reports');
const destination=path.join(reports,`install-check-${Date.now()}`);
assert(destination.startsWith(reports+path.sep));assert(!fs.existsSync(destination));
const run=(file,args,timeout=60000)=>new Promise((resolve,reject)=>{
 const child=spawn(file,args,{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe']});let output='';
 const timer=setTimeout(()=>{child.kill();reject(Error(`Timed out: ${file}`));},timeout);
 child.stdout.on('data',d=>{output+=d.toString();});child.stderr.on('data',d=>{output+=d.toString();});
 child.on('error',e=>{clearTimeout(timer);reject(e);});child.on('exit',code=>{clearTimeout(timer);if(code!==0)reject(Error(`${file} exited ${code}: ${output.slice(-5000)}`));else resolve(output);});
});
const installed=await run('powershell',['-NoProfile','-Command',"Get-ChildItem -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall' | Get-ItemProperty | Where-Object { $_.DisplayName -eq 'GenosStyleStudio' } | Select-Object -ExpandProperty DisplayName"]);
assert.equal(installed.trim(),'','A pre-existing installation must be preserved');
const setup=path.join(root,'target/release/bundle/nsis/GenosStyleStudio_0.1.0_x64-setup.exe');
let app;
try{
 await run(setup,['/S',`/D=${destination}`]);
 const exe=path.join(destination,'genos-style-studio.exe');assert(fs.existsSync(exe));assert(fs.existsSync(path.join(destination,'third-party/NOTICES.md')));
 const inventory=JSON.parse(fs.readFileSync(path.join(destination,'third-party/dependency-inventory.json'),'utf8'));
 assert(inventory.length>0);assert(inventory.every(r=>typeof r.license==='string'&&r.license.length));
 for(const record of inventory.filter(r=>r.license==='MPL-2.0')){if(record.ecosystem==='cargo'){assert(fs.existsSync(path.join(destination,'third-party/mpl-sources',`${record.name}-${record.version}`)));}else{assert.equal(record.dev,true,'Runtime MPL npm source obligations need separate review');}}
 const a=fs.readFileSync('target/release/genos-style-studio.exe'),b=fs.readFileSync(exe);const raw=Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK'),packaged=Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS');assert.equal(a.length,b.length);assert(a.indexOf(raw)>=0);assert.equal(a.indexOf(raw),b.indexOf(packaged));const normalized=Buffer.from(b);normalized.set(raw,b.indexOf(packaged));assert(a.equals(normalized),'Unexpected installed payload differences');
 app=spawn(exe,[],{cwd:destination,windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(reports,`webview-profile-${Date.now()}`),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-port=9450'}});
 // Wait for this freshly launched app's CDP endpoint, without opening any other app.
 let ready=false;for(let i=0;i<100;i++){try{const r=await fetch('http://127.0.0.1:9450/json/version');if(r.ok){ready=true;break;}}catch{}await new Promise(r=>setTimeout(r,100));}assert(ready);
 const smoke=await run(process.execPath,['scripts/release-smoke.mjs']);assert(smoke.includes('PACKAGED WINDOWS APP VERIFIED'));
 const midi=await run(process.execPath,['scripts/software-midi-smoke.mjs']);assert(midi.includes('SOFTWARE MIDI ROUTE VERIFIED'));
 app.kill();await new Promise(r=>setTimeout(r,250));app=undefined;
 await run(path.join(destination,'uninstall.exe'),['/S']);
 let removed=false;for(let i=0;i<100;i++){if(!fs.existsSync(exe)){removed=true;break;}await new Promise(r=>setTimeout(r,100));}assert(removed,'Installed executable was not removed');
 const remains=await run('powershell',['-NoProfile','-Command',"Get-ChildItem -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall' | Get-ItemProperty | Where-Object { $_.DisplayName -eq 'GenosStyleStudio' } | Select-Object -ExpandProperty DisplayName"]);assert.equal(remains.trim(),'');
 fs.writeFileSync('../docs/development/reports/installer-test-final.json',JSON.stringify({result:'INSTALLER LIFECYCLE VERIFIED',destination,installExit:0,uninstallExit:0,noticesInstalled:true,sourceTreesInstalled:fs.existsSync('../docs/development/third-party/mpl-sources'),appWorkflow:true,softwareMidiOnly:true,registryEntryRemoved:true,payloadDifference:'Only expected Tauri UNK→NSS bundle marker',limitations:['Current Windows host, not a clean VM or upgrade test.','No physical Yamaha device or audio capture.']},null,2));
 console.log('INSTALLER LIFECYCLE VERIFIED: isolated install, embedded app workflow, software MIDI, uninstall and registry cleanup');
}finally{if(app){app.kill();await new Promise(r=>setTimeout(r,250));}const uninstaller=path.join(destination,'uninstall.exe');if(fs.existsSync(uninstaller)){await run(uninstaller,['/S']);}}
