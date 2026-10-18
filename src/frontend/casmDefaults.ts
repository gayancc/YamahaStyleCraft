import defaults from "../crates/genos-core/assets/casm-part-defaults.json";

/** Draft the same named-field reset as native PartDefaults; retain opaque data. */
export function partDefaultBytes(bytes: number[]): number[] {
  if (bytes.length < 47 || bytes[9] < 8 || bytes[9] > 15)
    throw new Error("CASM part defaults require a Ctb2 destination 9–16");
  const row = defaults[bytes[9] - 8];
  const next = [...bytes];
  next[11] |= 15;
  next[12] = 255;
  next[13] = (next[13] & ~7) | 3 | (row[0] << 2);
  next.fill(255, 14, 18);
  next.splice(18, 4, 0, 2, 0, 127);
  for (const offset of [22, 28, 34]) next.splice(offset, 6, ...row.slice(1));
  return next;
}
