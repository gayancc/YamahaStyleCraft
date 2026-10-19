// Decode the installed StyleMagic Genos definition without executing the app.
import fs from "node:fs";
import crypto from "node:crypto";
import path from "node:path";
const source = process.argv[2];
if (!source) throw new Error("Pass the inspected GENOS.idl path");
const bytes = fs.readFileSync(source);
const hash = crypto.createHash("sha256").update(bytes).digest("hex");
if (hash !== "51e6586aa296ebb38f2366061f24793e303cbc1374fc4f024fa7150feec8fbc9") throw new Error("Unexpected Genos instrument definition");
const sections = new Map(); let section = "";
for (const line of bytes.toString("latin1").split(/\r?\n/)) {
  const match = line.match(/^\[([^\]]+)\]/);
  if (match) { section = match[1]; sections.set(section, []); }
  else if (section) sections.get(section).push(line);
}
const file = "crates/genos-core/data/genos-compatibility-voices.json";
const base = JSON.parse(fs.readFileSync("crates/genos-core/data/genos-factory-voices.json", "utf8"));
const known = new Set(base.voices.map(v => `${v.msb}:${v.lsb}:${v.program}`));
const families = ["Piano", "ChromaticPercussion", "Organ", "Guitar", "Bass", "Strings", "Ensemble", "Brass", "Reed", "Pipe", "SynthLead", "SynthPad", "SynthEffects", "Ethnic", "Percussion", "SoundEffects"];
const voices = [];
for (const type of ["Normal Voice", "Organ Flutes", "SFX Voice", "GM2 Normal", "GM2 Rhythm", "SFX Kit", "Drum Kit"]) {
  for (const line of sections.get(type) || []) {
    const match = line.match(/^Patch\[(\d+)\]=(.*)/); if (!match) continue;
    const bank = Number(match[1]), bankSection = match[2].trim();
    for (const row of sections.get(bankSection) || []) {
      const entry = row.match(/^(\d+)=(.+)/); if (!entry) continue;
      const msb = bank >> 7, lsb = bank & 127, program = Number(entry[1]), key = `${msb}:${lsb}:${program}`;
      if (known.has(key) || /^<[-]+>$/.test(entry[2].trim())) continue;
      known.add(key);
      if (![0, 64, 120, 121].includes(msb) || lsb > 127 || program > 127) throw new Error(`Unverified compatibility bank ${key}`);
      voices.push({ category: msb === 120 ? "GM2 DrumKit" : msb === 64 ? "SoundEffects" : families[program >> 3], subcategory: bankSection, name: entry[2].trim(), msb, lsb, program, voiceType: msb === 120 ? "GM2 Drums" : msb === 121 ? "GM2" : "XG", page: 0 });
    }
  }
}
if (voices.length !== 745 || known.size !== 2456) throw new Error("Incomplete Genos compatibility extraction");
fs.writeFileSync(file, JSON.stringify({ schema: "genos.compatibility-voices.v1", sourceSha256: hash, voices }));
fs.mkdirSync("crates/genos-core/tests/fixtures", { recursive: true });
fs.writeFileSync(path.join("crates/genos-core/tests/fixtures", "genos-stylemagic.idl"), bytes);
console.log(`GENOS COMPATIBILITY GENERATED: ${voices.length} additional addresses; ${known.size} total`);
