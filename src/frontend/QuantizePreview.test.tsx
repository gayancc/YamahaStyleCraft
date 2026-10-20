import { describe, expect, it } from "vitest";
import type { QuantizeOptions } from "./contracts";
import { gridLines, previewNotes } from "./QuantizePreview";

const base: QuantizeOptions = {
  target: "starts",
  mode: "yamaha",
  size: "120",
  soft: true,
  strength: 100,
  tuplet: 0,
  swing: 0,
  catchEnabled: false,
  catchPercent: 100,
  safeRange: 0,
  rough: 0,
  moveControllers: true,
  fromOriginal: false,
  sourceFiltered: false,
};

describe("quantize preview model", () => {
  it("draws a 1/16 grid with five lines per quarter note", () => {
    expect(gridLines(base)).toEqual([0, 120, 240, 360, 480]);
  });
  it("shifts odd lines for swing and ignores swing with a tuplet", () => {
    expect(gridLines({ ...base, swing: 50 })[1]).toBe(150);
    expect(gridLines({ ...base, swing: 50, tuplet: 3 })).toEqual([0, 80, 160, 240, 320, 400, 480]);
  });
  it("snaps fully at 100% and part-way at lower strength", () => {
    expect(previewNotes(base).map((n) => n.snapped)).toEqual([0, 120, 240, 360]);
    expect(previewNotes({ ...base, strength: 50 })[0].snapped).toBe(17);
    expect(previewNotes({ ...base, soft: false, strength: 50 })[0].snapped).toBe(0);
  });
  it("leaves notes alone outside the catch range or inside the safe range", () => {
    expect(previewNotes({ ...base, catchEnabled: true, catchPercent: 10 })[0].snapped).toBe(34);
    expect(previewNotes({ ...base, safeRange: 40 })[0].snapped).toBe(34);
  });
});
