import { describe, expect, it } from "vitest";
import { defaultSlotBars, layoutSections, slotKeyFor } from "./sectionSlots";

const sec = (id: number, label: string) => ({
  id,
  label,
  start: id * 1000,
  end: id * 1000 + 960,
});

describe("slotKeyFor", () => {
  it("maps Yamaha letter names onto the Genos panel numbering", () => {
    expect(slotKeyFor("Intro A")).toBe("intro1");
    expect(slotKeyFor("Intro B")).toBe("intro2");
    expect(slotKeyFor("Intro C")).toBe("intro3");
    expect(slotKeyFor("Ending A")).toBe("ending1");
    expect(slotKeyFor("Ending C")).toBe("ending3");
  });
  it("accepts numbered names and is case/space tolerant", () => {
    expect(slotKeyFor("Intro 2")).toBe("intro2");
    expect(slotKeyFor("  main   b ")).toBe("mainB");
    expect(slotKeyFor("ENDING 3")).toBe("ending3");
  });
  it("maps per-main fills and the break fill", () => {
    expect(slotKeyFor("Fill In AA")).toBe("fillA");
    expect(slotKeyFor("Fill In BB")).toBe("fillB");
    expect(slotKeyFor("Fill In DD")).toBe("fillD");
    expect(slotKeyFor("Fill In BA")).toBe("break");
    expect(slotKeyFor("Break")).toBe("break");
  });
  it("returns null for names that have no Genos panel slot", () => {
    expect(slotKeyFor("Intro D")).toBeNull();
    expect(slotKeyFor("Main E")).toBeNull();
    expect(slotKeyFor("Fill In AB")).toBeNull();
    expect(slotKeyFor("SInt")).toBeNull();
  });
});

describe("slot metadata", () => {
  it("carries the Yamaha marker name that adding the slot writes", () => {
    const slots = layoutSections([]).groups.flatMap((g) => g.slots);
    expect(slots.map((s) => [s.label, s.yamahaLabel])).toEqual([
      ["Intro 1", "Intro A"],
      ["Intro 2", "Intro B"],
      ["Intro 3", "Intro C"],
      ["Main A", "Main A"],
      ["Main B", "Main B"],
      ["Main C", "Main C"],
      ["Main D", "Main D"],
      ["Fill A", "Fill In AA"],
      ["Fill B", "Fill In BB"],
      ["Fill C", "Fill In CC"],
      ["Fill D", "Fill In DD"],
      ["Break", "Fill In BA"],
      ["Ending 1", "Ending A"],
      ["Ending 2", "Ending B"],
      ["Ending 3", "Ending C"],
    ]);
  });
  it("defaults lengths like SFM Style Studio and bounds fill and break to 8 bars", () => {
    expect(defaultSlotBars("Main")).toEqual({ bars: 4, max: 32 });
    expect(defaultSlotBars("Fill")).toEqual({ bars: 1, max: 8 });
    expect(defaultSlotBars("Break")).toEqual({ bars: 1, max: 8 });
    expect(defaultSlotBars("Intro")).toEqual({ bars: 4, max: 32 });
    expect(defaultSlotBars("Ending")).toEqual({ bars: 4, max: 32 });
  });
});

describe("layoutSections", () => {
  it("always lists every Genos slot in panel order, empty when the Style has no data", () => {
    const layout = layoutSections([]);
    expect(layout.groups.map((g) => g.name)).toEqual([
      "Intro",
      "Main",
      "Fill",
      "Break",
      "Ending",
    ]);
    expect(layout.groups.map((g) => g.slots.map((s) => s.label))).toEqual([
      ["Intro 1", "Intro 2", "Intro 3"],
      ["Main A", "Main B", "Main C", "Main D"],
      ["Fill A", "Fill B", "Fill C", "Fill D"],
      ["Break"],
      ["Ending 1", "Ending 2", "Ending 3"],
    ]);
    expect(layout.groups.flatMap((g) => g.slots).every((s) => s.section === null)).toBe(true);
    expect(layout.extra).toEqual([]);
  });

  it("fills only the slots the loaded Style provides", () => {
    const sections = [
      sec(1, "Intro B"),
      sec(2, "Main A"),
      sec(3, "Main C"),
      sec(4, "Fill In AA"),
      sec(5, "Ending A"),
    ];
    const slots = layoutSections(sections).groups.flatMap((g) => g.slots);
    const filled = slots.filter((s) => s.section).map((s) => [s.key, s.section!.id]);
    expect(filled).toEqual([
      ["intro2", 1],
      ["mainA", 2],
      ["mainC", 3],
      ["fillA", 4],
      ["ending1", 5],
    ]);
    expect(slots.find((s) => s.key === "mainB")!.section).toBeNull();
  });

  it("keeps unrecognized or duplicate sections reachable instead of hiding data", () => {
    const layout = layoutSections([
      sec(1, "Main A"),
      sec(2, "Main A"),
      sec(3, "Intro D"),
    ]);
    expect(layout.groups[1].slots[0].section!.id).toBe(1);
    expect(layout.extra.map((s) => s.id)).toEqual([2, 3]);
  });
});
