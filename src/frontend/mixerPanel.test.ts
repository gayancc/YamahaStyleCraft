import { describe, expect, it } from "vitest";
import { pitchBendRangeSemitones } from "./MixerPanel";

describe("pitchBendRangeSemitones", () => {
  it("reads Genos RPN 0,0 data-entry semitones", () => {
    const status = 0xb0 | 10; // CH11
    const events = [
      { id: 1, tick: 100, order: 1, data: [status, 101, 0] },
      { id: 2, tick: 100, order: 2, data: [status, 100, 0] },
      { id: 3, tick: 100, order: 3, data: [status, 6, 12] },
      { id: 4, tick: 100, order: 4, data: [status, 101, 127] },
      { id: 5, tick: 100, order: 5, data: [status, 100, 127] },
    ];
    expect(pitchBendRangeSemitones(events, 11, 100)).toBe(12);
    expect(pitchBendRangeSemitones(events, 11, 99)).toBeNull();
    expect(pitchBendRangeSemitones(events, 12, 100)).toBeNull();
  });
});
