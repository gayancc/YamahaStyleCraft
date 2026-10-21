import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { CatalogVoice } from "./contracts";
import { useStudio } from "./store";

export interface InstalledVoice {
  pack: string;
  name: string;
  kit: boolean;
  msb: number;
  lsb: number;
  program: number;
}

export interface InstalledPacks {
  sourcePath: string;
  voices: InstalledVoice[];
  skipped: number;
}

export const EXPANSION_KIT = "ExpansionKit";
export const EXPANSION_VOICE = "ExpansionVoice";
export const isKit = (v: CatalogVoice) => /kit/i.test(v.voiceType);

const key = (v: Pick<CatalogVoice, "msb" | "lsb" | "program">) =>
  `${v.msb}-${v.lsb}-${v.program}`;

/** Merge voice lists; the first list wins when two entries share an address. */
export function mergeVoices(...lists: CatalogVoice[][]): CatalogVoice[] {
  const seen = new Set<string>();
  const out: CatalogVoice[] = [];
  for (const list of lists)
    for (const v of list) {
      const k = key(v);
      if (seen.has(k)) continue;
      seen.add(k);
      out.push(v);
    }
  return out;
}

export function toCatalogVoices(packs: InstalledPacks): CatalogVoice[] {
  return packs.voices.map((v) => ({
    category: "Expansion",
    subcategory: v.pack || "-",
    name: v.name,
    msb: v.msb,
    lsb: v.lsb,
    program: v.program,
    voiceType: v.kit ? EXPANSION_KIT : EXPANSION_VOICE,
    page: 0,
  }));
}

/** Replace the installed-pack voices, keeping any voices recovered from other sources. */
function apply(packs: InstalledPacks) {
  const { expansionVoices, setExpansionVoices } = useStudio.getState();
  const others = expansionVoices.filter(
    (v) => v.voiceType !== EXPANSION_KIT && v.voiceType !== EXPANSION_VOICE,
  );
  setExpansionVoices(mergeVoices(toCatalogVoices(packs), others));
}

/** Load the remembered installed packs (re-read from disk when the file is still there). */
export async function loadInstalledPacks(): Promise<void> {
  if (!isTauri()) return;
  try {
    const packs = await invoke<InstalledPacks | null>("load_installed_packs");
    if (packs) apply(packs);
  } catch {
    // The picker still works with the factory catalog.
  }
}

/** Ask for the Expansion Manager installation file and import every voice and kit from it. */
export async function importInstalledPacks(): Promise<InstalledPacks | null> {
  const path = await open({
    multiple: false,
    title: "Choose the Pack Install Data file (e.g. Genos_PackInstallData.pqi)",
    filters: [{ name: "Pack install data", extensions: ["pqi", "ppi"] }],
  });
  if (typeof path !== "string") return null;
  const packs = await invoke<InstalledPacks>("scan_installed_packs", { path });
  apply(packs);
  return packs;
}
