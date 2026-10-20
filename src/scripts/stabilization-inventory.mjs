// Source inventory, not a claim that every discovered control passed E2E QA.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import ts from "typescript";
import {fileURLToPath} from "node:url";
const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const root = path.dirname(workspace);
const out = path.join(root, "docs/development/stabilization");
fs.mkdirSync(out, {recursive:true});
const walk = dir => fs.readdirSync(dir,{withFileTypes:true}).flatMap(e => {
  if (["node_modules","target","dist","icons","third-party","reports","releases"].includes(e.name)) return [];
  const file=path.join(dir,e.name); return e.isDirectory()?walk(file):[file];
});
const files=walk(workspace).filter(f=>/\.(rs|tsx?|mjs|toml|json|css|html)$/.test(f));
const inventory=[], controls=[], calls=[], tests=[];
for(const file of files){
  const source=fs.readFileSync(file,"utf8"), relative=path.relative(root,file).replaceAll("\\","/");
  inventory.push({path:relative,lines:source.split(/\r?\n/).length,sha256:crypto.createHash("sha256").update(source).digest("hex")});
  if(/frontend.*\.tsx$/.test(file)&&!file.includes(".test.")){
    const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
    const visit=n=>{
      if(ts.isJsxOpeningElement(n)||ts.isJsxSelfClosingElement(n)){
        const tag=n.tagName.getText(ast);
        if(["button","input","select","textarea","summary","IconButton"].includes(tag)){
          const attrs=n.attributes.properties.map(p=>p.getText(ast));
          controls.push({file:relative,line:ast.getLineAndCharacterOfPosition(n.getStart(ast)).line+1,tag,attributes:attrs});
        }
      }
      ts.forEachChild(n,visit);
    };visit(ast);
  }
  for(const m of source.matchAll(/invoke(?:<[^;]*?>)?\s*\(\s*["']([a-z_]+)["']/g)) calls.push({command:m[1],file:relative,line:source.slice(0,m.index).split("\n").length});
  if(/\.test\.|\/tests\//.test(relative))tests.push({file:relative,names:[...source.matchAll(/(?:fn\s+([a-z_]\w*)\s*\(|\bit\(\s*["']([^"']+))/g)].map(m=>m[1]??m[2])});
}
const main=fs.readFileSync(path.join(workspace,"src-tauri/src/main.rs"),"utf8");
const commands=[...main.matchAll(/#\[tauri::command\][\s\S]*?\b(?:async\s+)?fn\s+(\w+)/g)].map(m=>m[1]);
const fixtures=["docs/research/experiments/fixtures/08.3-4-ACOUSTIC-control.STY","docs/research/experiments/expansion-fixtures/mini-install.ppi","docs/research/experiments/expansion-fixtures/univ-tiny.ppf","docs/research/Genos_InstrumentInfo.n27"].map(f=>({path:f,exists:fs.existsSync(path.join(root,f)),sha256:fs.existsSync(path.join(root,f))?crypto.createHash("sha256").update(fs.readFileSync(path.join(root,f))).digest("hex"):null}));
const report={created:new Date().toISOString(),inventory,controls,commands,calls,tests,fixtures,limitations:["Static discovery only; dynamic controls and individual interactions require review.","A matched test name or IPC call does not prove a passing UI workflow.","Research fixtures are local and excluded from Git."]};
fs.writeFileSync(path.join(out,"source-inventory.json"),JSON.stringify(report,null,2));
const rows=Object.groupBy(controls,c=>c.file);
fs.writeFileSync(path.join(out,"control-inventory.md"),"# Discovered UI controls\n\nEvery row remains subject to individual E2E verification. Detailed attributes and source hashes are in source-inventory.json.\n\n| Component | Static controls | Current verification |\n|---|---:|---|\n"+Object.entries(rows).map(([file,cs])=>`| ${file} | ${cs.length} | Consult native evidence and traceability; no blanket pass |`).join("\n")+"\n");
fs.writeFileSync(path.join(out,"ipc-traceability.md"),"# Native command traceability\n\nStatic associations, not execution results. Indirect/dynamic calls need manual review.\n\n| Command | UI callers | Test/tool callers | Actual result and gap |\n|---|---|---|---|\n"+commands.map(command=>{
  const refs=calls.filter(c=>c.command===command);
  return `| ${command} | ${refs.filter(c=>c.file.includes("frontend/")&&!c.file.includes(".test.")).map(c=>`${c.file}:${c.line}`).join(", ")||"Internal or indirect; review required"} | ${refs.filter(c=>c.file.includes("scripts/")||c.file.includes(".test.")).map(c=>`${c.file}:${c.line}`).join(", ")||"No literal invoke match; review required"} | See workflow traceability and fresh run logs; no blanket pass |`;
}).join("\n")+"\n");
console.log(`INVENTORY WRITTEN: ${inventory.length} source/configuration files, ${controls.length} static controls, ${commands.length} native commands, ${tests.length} test files`);
