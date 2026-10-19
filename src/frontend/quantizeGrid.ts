import type { QuantizeGrid } from "./contracts";

// Grid names use PPQ480 units, while source MIDI retains its actual PPQ.
export function quantizeGridForTicks(ticks: number, ppq: number): QuantizeGrid {
  const scaled = Math.round((ticks * 480) / ppq);
  return [480, 320, 240, 160, 120, 80, 60, 40].includes(scaled)
    ? (String(scaled) as QuantizeGrid)
    : "120";
}
