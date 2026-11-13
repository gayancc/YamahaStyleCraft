import { describe, expect, it } from "vitest";
import { chordTypeLabel } from "./chordTypes";

describe("chordTypeLabel", () => {
  it("names Genos firmware chord types with Yamaha chord names", () => {
    expect(chordTypeLabel("Genos:1")).toBe("6");
    expect(chordTypeLabel("Genos:20")).toBe("7sus4");
    expect(chordTypeLabel("Genos:31")).toBe("1+5");
  });

  it("keeps supported qualities and labels unknown IDs", () => {
    expect(chordTypeLabel("m7")).toBe("m7");
    expect(chordTypeLabel("Genos:50")).toBe("type 50");
  });
});
