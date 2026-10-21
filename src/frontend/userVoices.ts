import type { CatalogVoice } from "./contracts";

const KEY = "genos.userVoices.v1";
export const USER_VOICE_CATEGORY = "My voices";

/** Kinds a user voice can be filed under; "kit" voices are the drum / SFX kits. */
export type UserVoiceKind = "voice" | "kit";

const inRange = (n: unknown, max: number): n is number =>
  typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= max;

/** Keep only well-formed entries so a damaged or hand-edited store can never break the picker. */
export function sanitizeUserVoices(raw: unknown): CatalogVoice[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const out: CatalogVoice[] = [];
  for (const v of raw) {
    if (!v || typeof v !== "object") continue;
    const e = v as Record<string, unknown>;
    if (typeof e.name !== "string" || !e.name.trim()) continue;
    if (
      !inRange(e.msb, 127) ||
      !inRange(e.lsb, 127) ||
      !inRange(e.program, 127)
    )
      continue;
    const key = `${e.msb}-${e.lsb}-${e.program}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({
      category: USER_VOICE_CATEGORY,
      subcategory:
        typeof e.subcategory === "string" && e.subcategory.trim()
          ? e.subcategory.trim()
          : "-",
      name: e.name.trim().slice(0, 60),
      msb: e.msb,
      lsb: e.lsb,
      program: e.program,
      voiceType: e.voiceType === "UserKit" ? "UserKit" : "UserVoice",
      page: 0,
    });
  }
  return out;
}

export function loadUserVoices(): CatalogVoice[] {
  try {
    return sanitizeUserVoices(JSON.parse(localStorage.getItem(KEY) ?? "[]"));
  } catch {
    return [];
  }
}

export function saveUserVoices(voices: CatalogVoice[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(voices));
  } catch {
    // Storage can be unavailable; the list still works for this session.
  }
}

export const isUserVoice = (v: CatalogVoice) =>
  v.voiceType === "UserVoice" || v.voiceType === "UserKit";
