import { describe, expect, it } from "vitest";
import {
  estimateMidiLevel,
  holdMidiPeak,
  midiLevelBand,
  midiLevelSegments,
} from "./midiLevel";

describe("midiLevel", () => {
  it("estimates velocity × volume × expression at playhead for audible parts", () => {
    const notes = [
      { channel: 10, start: 0, duration: 100, velocity: 127 },
      { channel: 11, start: 0, duration: 100, velocity: 64 },
    ];
    expect(
      estimateMidiLevel(notes, 50, [10, 11], () => 127),
    ).toBeCloseTo(1, 5);
    expect(
      estimateMidiLevel(notes, 50, [11], () => 127),
    ).toBeCloseTo(64 / 127, 5);
    expect(estimateMidiLevel(notes, 50, [10], () => 64)).toBeCloseTo(
      127 / 127 * (64 / 127),
      5,
    );
    expect(
      estimateMidiLevel(notes, 50, [10], () => 127, () => 64),
    ).toBeCloseTo(64 / 127, 5);
    expect(estimateMidiLevel(notes, null, [10], () => 127)).toBe(0);
  });

  it("maps SFM hot/warn/safe bands", () => {
    expect(midiLevelBand(0.5)).toBe("safe");
    expect(midiLevelBand(0.72)).toBe("warn");
    expect(midiLevelBand(0.88)).toBe("hot");
  });

  it("holds peak for 700ms then falls", () => {
    const up = holdMidiPeak(0, 0, 0.9, 100);
    expect(up.peak).toBeCloseTo(0.9);
    const held = holdMidiPeak(up.peak, up.peakAtMs, 0.1, 500);
    expect(held.peak).toBeCloseTo(0.9);
    const fallen = holdMidiPeak(held.peak, held.peakAtMs, 0.1, 900);
    expect(fallen.peak).toBeCloseTo(0.1);
  });

  it("builds 20 LED segments with peak marker", () => {
    const segs = midiLevelSegments(0.5, 0.9);
    expect(segs).toHaveLength(20);
    expect(segs.filter((s) => s.lit)).toHaveLength(10);
    expect(segs.filter((s) => s.peak)).toHaveLength(1);
  });
});
