import { describe, expect, it } from "vitest";
import { sectionSortKey, sortSections } from "./sectionOrder";

describe("sectionOrder", () => {
  it("orders Intro → Main → Fill → Break → Ending", () => {
    const sorted = sortSections([
      { label: "Ending A" },
      { label: "Main B" },
      { label: "Break" },
      { label: "Intro A" },
      { label: "Fill In AA" },
      { label: "Main A" },
    ]).map((s) => s.label);
    expect(sorted).toEqual([
      "Intro A",
      "Main A",
      "Main B",
      "Fill In AA",
      "Break",
      "Ending A",
    ]);
  });

  it("ranks unknown labels last", () => {
    expect(sectionSortKey("Pattern")[0]).toBe(5);
    expect(sectionSortKey("Intro A")[0]).toBe(0);
  });
});
