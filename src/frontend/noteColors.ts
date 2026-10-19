/** Per-source display colors (SFM Tools → Note colors). Display-only; never MIDI. */

/** CH1–8 generic; CH9–16 match SFM V2.0.44 role defaults (Rhy1…Phrase2). */
export const DEFAULT_NOTE_COLORS: string[] = [
  "#5c8a6e", // CH1
  "#6a7f9a",
  "#8a6b5c",
  "#7a6a9a",
  "#6a9a8a",
  "#9a8a5c",
  "#5c7a9a",
  "#9a6a7a",
  "#20d9f3", // CH9 Rhy1 (SFM)
  "#3978f6", // CH10 Rhy2
  "#f7bd42", // CH11 Bass
  "#34d399", // CH12 Chord1
  "#2dd4bf", // CH13 Chord2
  "#906ef7", // CH14 Pad
  "#fb7185", // CH15 Phrase1
  "#f472b6", // CH16 Phrase2
];

const HEX = /^#[0-9a-fA-F]{6}$/;

/** Sanitize 16 channel colors; pad/truncate; invalid → default. */
export function sanitizeNoteColors(input: unknown): string[] {
  const raw = Array.isArray(input) ? input : [];
  return DEFAULT_NOTE_COLORS.map((fallback, i) => {
    const v = raw[i];
    return typeof v === "string" && HEX.test(v) ? v : fallback;
  });
}

/** Mix channel hue with velocity lightness (SFM: notes + velocity update together). */
export function noteFill(channelColor: string, velocity: number, selected: boolean): string {
  if (selected) return "#d98d96";
  const v = Math.max(1, Math.min(127, velocity));
  const light = 0.72 + (v / 127) * 0.28;
  return mixLight(channelColor, light);
}

function mixLight(hex: string, amount: number): string {
  const n = parseInt(hex.slice(1), 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  const mix = (c: number) => Math.round(c * amount + 255 * (1 - amount) * 0.08);
  return `rgb(${mix(r)},${mix(g)},${mix(b)})`;
}
