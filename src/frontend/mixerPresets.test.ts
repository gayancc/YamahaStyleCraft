import { describe, expect, it } from "vitest";
import { SAFE_FX_PRESETS } from "./mixerPresets";

describe("SAFE_FX_PRESETS", () => {
  it("covers the five observed SFM Safe FX keys with CC-only values", () => {
    expect(SAFE_FX_PRESETS.map((p) => p.key)).toEqual([
      "dry",
      "tight",
      "warm",
      "wide",
      "bright",
    ]);
    for (const p of SAFE_FX_PRESETS) {
      expect(p.values).toHaveLength(4);
      for (const { controller, value } of p.values) {
        expect([71, 74, 91, 93]).toContain(controller);
        expect(value).toBeGreaterThanOrEqual(0);
        expect(value).toBeLessThanOrEqual(127);
      }
    }
  });

  it("maps dry to zero sends and neutral filter", () => {
    const dry = SAFE_FX_PRESETS.find((p) => p.key === "dry")!;
    const byCc = Object.fromEntries(dry.values.map((v) => [v.controller, v.value]));
    expect(byCc[91]).toBe(0);
    expect(byCc[93]).toBe(0);
    expect(byCc[74]).toBe(64);
    expect(byCc[71]).toBe(64);
  });
});
