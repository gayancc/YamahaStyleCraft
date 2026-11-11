import type { Region } from "./contracts";

const signedZone = (byte: number) => (byte >= 128 ? byte - 256 : byte);

export function importedOutputLimitDescription(low: number, high: number): string | null {
  const lower = low & 127;
  const upper = high & 127;
  if (((low | high) & 128) === 0 && lower <= upper) return null;
  const kind = (byte: number) => byte & 128 ? "hard cutoff" : "octave fold";
  return `Imported output limits: low ${lower} (${kind(low)}), high ${upper} (${kind(high)})${lower > upper ? " · inverted" : ""}.`;
}

export function sourceRegionOffset(pitch: number, low: number, high: number): number {
  return pitch < signedZone(low) ? 22 : pitch > signedZone(high) ? 34 : 28;
}

export function sourceRegionRange(
  lowByte: number,
  highByte: number,
  region: Region,
): [number, number] | null {
  if (![lowByte, highByte].every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) {
    return null;
  }
  const low = signedZone(lowByte);
  const high = signedZone(highByte);
  // The low branch has priority when the imported zone is inverted.
  const [start, end] =
    region === "low"
      ? [0, low - 1]
      : region === "high"
        ? [Math.max(0, low, high + 1), 127]
        : [Math.max(0, low), high];
  return start <= end ? [start, end] : null;
}
