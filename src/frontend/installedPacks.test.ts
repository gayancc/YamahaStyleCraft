import { describe, expect, it } from "vitest";
import { isKit, mergeVoices, toCatalogVoices } from "./installedPacks";

describe("installed packs", () => {
  const packs = {
    sourcePath: "x.pqi",
    skipped: 0,
    voices: [
      { pack: "Brass", name: "brass fall", kit: false, msb: 63, lsb: 2, program: 0 },
      { pack: "Kits", name: "7-8 Kit", kit: true, msb: 62, lsb: 0, program: 14 },
    ],
  };

  it("maps installed voices and kits to picker entries named after the pack", () => {
    const [voice, kit] = toCatalogVoices(packs);
    expect(voice).toMatchObject({ category: "Expansion", subcategory: "Brass", name: "brass fall" });
    expect(isKit(voice)).toBe(false);
    expect(isKit(kit)).toBe(true);
    expect(kit.program).toBe(14);
  });

  it("merges lists without duplicating an address, first list winning", () => {
    const a = toCatalogVoices(packs);
    const b = [{ ...a[0], name: "other" }];
    const merged = mergeVoices(a, b);
    expect(merged).toHaveLength(2);
    expect(merged[0].name).toBe("brass fall");
  });
});
