import { describe, expect, it } from "vitest";
import {
  DEFAULT_NOTE_COLORS,
  noteFill,
  sanitizeNoteColors,
} from "./noteColors";

describe("noteColors", () => {
  it("sanitizes length and invalid entries", () => {
    expect(sanitizeNoteColors(null)).toEqual(DEFAULT_NOTE_COLORS);
    expect(sanitizeNoteColors(["#ff0000"])[0]).toBe("#ff0000");
    expect(sanitizeNoteColors(["#ff0000"])[1]).toBe(DEFAULT_NOTE_COLORS[1]);
    expect(sanitizeNoteColors(["nope"])[0]).toBe(DEFAULT_NOTE_COLORS[0]);
  });

  it("keeps selection highlight distinct", () => {
    expect(noteFill("#5c8a6e", 90, true)).toBe("#d98d96");
    expect(noteFill("#5c8a6e", 90, false)).toMatch(/^rgb\(/);
  });
});
