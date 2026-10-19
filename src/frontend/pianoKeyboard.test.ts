import { describe, expect, it } from "vitest";
import { pianoKeyAt, pianoKeyFaces } from "./pianoKeyboard";

describe("vertical piano keyboard", () => {
  it("uses seven natural faces with 1.5/2-row spans per octave", () => {
    const keys = pianoKeyFaces(18, 70);
    const heights = [60, 62, 64, 65, 67, 69, 71].map((pitch) => {
      const key = keys.find((k) => k.pitch === pitch)!;
      return key.bottom - key.top;
    });
    expect(heights).toEqual([27, 36, 27, 27, 36, 36, 27]);
    expect(keys.find((k) => k.pitch === 60)).toMatchObject({
      top: 1197,
      bottom: 1224,
      right: 69,
    });
    expect(keys.find((k) => k.pitch === 61)).toMatchObject({
      top: 1188,
      bottom: 1206,
      right: 44,
    });
    expect(keys.slice(-5).every((k) => k.black)).toBe(true);
  });

  it("checks the accidental first and resolves its white tail by position", () => {
    expect(pianoKeyAt(10, 66.25 * 18, 18, 60)).toBe(61);
    expect(pianoKeyAt(10, 66.75 * 18, 18, 60)).toBe(61);
    expect(pianoKeyAt(50, 66.25 * 18, 18, 60)).toBe(62);
    expect(pianoKeyAt(50, 66.75 * 18, 18, 60)).toBe(60);
    expect(pianoKeyAt(50, 68 * 18, 18, 60)).toBe(59);
    expect(pianoKeyAt(50, 63 * 18, 18, 60)).toBe(64);
  });

  for (const row of [10, 11, 18, 23, 30]) {
    it(`has no gaps or overlaps between natural faces at row height ${row}`, () => {
      const whites = pianoKeyFaces(row, 60)
        .filter((k) => !k.black)
        .sort((a, b) => a.top - b.top);
      expect(whites[0].top).toBe(0);
      expect(whites.at(-1)!.bottom).toBe(128 * row);
      for (let i = 1; i < whites.length; i++)
        expect(whites[i].top).toBe(whites[i - 1].bottom);
      for (let pitch = 0; pitch < 128; pitch++) {
        // All semitone centers on the left agree with note-grid pitches.
        expect(pianoKeyAt(10, (127.5 - pitch) * row, row, 60)).toBe(pitch);
      }
      for (const white of whites) {
        expect(pianoKeyAt(58, white.top + 0.1, row, 60)).toBe(white.pitch);
        expect(pianoKeyAt(58, white.bottom - 0.1, row, 60)).toBe(white.pitch);
      }
    });
  }

  it("rejects the ruler, lower overflow and keyboard edge", () => {
    expect(pianoKeyAt(10, -0.1, 18, 60)).toBeNull();
    expect(pianoKeyAt(10, 128 * 18, 18, 60)).toBeNull();
    expect(pianoKeyAt(-1, 10, 18, 60)).toBeNull();
    expect(pianoKeyAt(60, 10, 18, 60)).toBeNull();
    expect(pianoKeyAt(58, 0, 18, 60)).toBe(127);
    expect(pianoKeyAt(58, 128 * 18 - 0.1, 18, 60)).toBe(0);
  });
});
