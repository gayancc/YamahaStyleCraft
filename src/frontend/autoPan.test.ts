import { describe, expect, it } from "vitest";
import { autoPanPoints } from "./autoPan";

describe("autoPanPoints", () => {
  it("generates sine pan within 1..127 over the section", () => {
    const pts = autoPanPoints({
      lengthTicks: 1920,
      ppq: 480,
      beats: 1,
      shape: "sine",
      depth: 100,
      sectionStart: 1000,
    });
    expect(pts.length).toBeGreaterThan(4);
    expect(pts[0].tick).toBe(1000);
    for (const p of pts) {
      expect(p.value).toBeGreaterThanOrEqual(1);
      expect(p.value).toBeLessThanOrEqual(127);
      expect(p.tick).toBeGreaterThanOrEqual(1000);
      expect(p.tick).toBeLessThan(1000 + 1920);
    }
  });

  it("square uses half-period strides and reverse flips side", () => {
    const a = autoPanPoints({
      lengthTicks: 1920,
      ppq: 480,
      beats: 2,
      shape: "square",
      depth: 100,
      sectionStart: 0,
    });
    const b = autoPanPoints({
      lengthTicks: 1920,
      ppq: 480,
      beats: 2,
      shape: "square",
      depth: 100,
      reverse: true,
      sectionStart: 0,
    });
    expect(a[0].value).not.toBe(b[0].value);
  });

  it("rejects invalid timing", () => {
    expect(() =>
      autoPanPoints({
        lengthTicks: 0,
        ppq: 480,
        beats: 1,
        shape: "sine",
        depth: 50,
        sectionStart: 0,
      }),
    ).toThrow(/Invalid Auto Pan/);
  });
});
