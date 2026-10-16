// Representative-file rendering benchmark; uses actual native parse, no mock music.
import {chromium} from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const browser=await chromium.connectOverCDP('http://127.0.0.1:9449');
try{
 const page=browser.contexts()[0].pages()[0];await page.reload();await page.waitForSelector('.brand');
 if(await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('recovery_status'))){await page.getByRole('button',{name:'Restore session',exact:true}).click();}
 const source=path.resolve('../docs/research/artifacts/style files/19.LATIN AFRO.T236.STY');
 const load=await page.evaluate(async source=>{const {useStudio}=await import('/frontend/store.ts');const begin=performance.now();await useStudio.getState().load(source);return {loadMs:performance.now()-begin,notes:useStudio.getState().view.notes.length};},source);
 assert.equal(load.notes,5695);
 const metrics=await page.evaluate(async()=>{
  const {useStudio}=await import('/frontend/store.ts');const v=useStudio.getState().view;
  const bySection=new Map(v.sections.map(s=>[s.id,v.notes.filter(n=>n.start>=s.start&&n.start<s.end)]));
  const sec=[...bySection.entries()].sort((a,b)=>b[1].length-a[1].length)[0];useStudio.getState().section(sec[0]);
  const counts=Array.from({length:16},(_,i)=>({channel:i+1,notes:sec[1].filter(n=>n.channel===i+1).length})).sort((a,b)=>b.notes-a.notes);useStudio.getState().setChannel(counts[0].channel);
  const samples=[];const next=()=>new Promise(r=>requestAnimationFrame(r));await next();await next();
  for(let i=0;i<60;i++){const before=performance.now();const scroll=document.querySelector('.piano-scroll');scroll.scrollLeft=i%2===0?0:scroll.scrollWidth/3;useStudio.setState({zoom:i%2===0?1:1.5});await next();await next();samples.push(performance.now()-before);}
  useStudio.setState({zoom:1});samples.sort((a,b)=>a-b);
  return {samples:60,medianResponseMs:samples[30],p95ResponseMs:samples[57],maxResponseMs:samples[59],selectedSectionNotes:sec[1].length,selectedChannelNotes:counts[0].notes,canvasCount:document.querySelectorAll('canvas').length,domElements:document.querySelectorAll('*').length,method:'Each sample changes viewport and zoom, then waits two actual animation frames. Native WebView2 on this host; no universal performance guarantee.'};
 });
 assert.equal(metrics.canvasCount,2);assert(metrics.p95ResponseMs<100,'Representative viewport/zoom response must stay below 100ms');
 const report={result:'REPRESENTATIVE RENDER BENCHMARK VERIFIED',source,...load,...metrics,pixiDecision:'Viewport canvas meets the measured representative-file gate; PixiJS is deferred until larger synthetic/real projects justify it.'};fs.writeFileSync('../docs/development/reports/render-benchmark.json',JSON.stringify(report,null,2));await page.screenshot({path:'../docs/development/reports/representative-workspace.png'});console.log(JSON.stringify(report,null,2));
}finally{await browser.close();}
