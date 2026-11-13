/** Yamaha chord-type names for Genos chord type IDs (XF numbering). */
const GENOS_TYPE_NAMES: Record<number, string> = {
  1: "6",
  3: "Maj7(#11)",
  4: "add9",
  5: "Maj7(9)",
  6: "6(9)",
  9: "m6",
  12: "m(9)",
  13: "m7(9)",
  14: "m7(11)",
  15: "mM7",
  16: "mM7(9)",
  18: "dim7",
  20: "7sus4",
  21: "7b5",
  22: "7(9)",
  23: "7(#11)",
  24: "7(13)",
  25: "7(b9)",
  26: "7(b13)",
  27: "7(#9)",
  28: "Maj7aug",
  29: "7aug",
  30: "1+8",
  31: "1+5",
};

/** Display name for an app chord quality, including exact `Genos:<id>` firmware types. */
export function chordTypeLabel(quality: string): string {
  const match = /^Genos:(\d+)$/.exec(quality);
  if (!match) return quality;
  return GENOS_TYPE_NAMES[Number(match[1])] ?? `type ${match[1]}`;
}
