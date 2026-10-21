import { describe, expect, it } from "vitest";
import { sanitizeUserVoices } from "./userVoices";

describe("sanitizeUserVoices", () => {
  it("keeps valid voices and kits, normalising their category", () => {
    const out = sanitizeUserVoices([
      { name: " Slap Bass ", msb: 104, lsb: 5, program: 12, voiceType: "UserVoice" },
      { name: "Rock Kit", msb: 127, lsb: 0, program: 3, voiceType: "UserKit" },
    ]);
    expect(out.map((v) => [v.name, v.category, v.voiceType])).toEqual([
      ["Slap Bass", "My voices", "UserVoice"],
      ["Rock Kit", "My voices", "UserKit"],
    ]);
  });

  it("drops malformed, out-of-range and duplicate entries", () => {
    const out = sanitizeUserVoices([
      null,
      { name: "", msb: 0, lsb: 0, program: 0 },
      { name: "Bad", msb: 128, lsb: 0, program: 0 },
      { name: "Frac", msb: 1.5, lsb: 0, program: 0 },
      { name: "Ok", msb: 1, lsb: 2, program: 3 },
      { name: "Dup", msb: 1, lsb: 2, program: 3 },
    ]);
    expect(out.map((v) => v.name)).toEqual(["Ok"]);
  });

  it("returns an empty list for non-array input", () => {
    expect(sanitizeUserVoices({ not: "a list" })).toEqual([]);
  });
});
