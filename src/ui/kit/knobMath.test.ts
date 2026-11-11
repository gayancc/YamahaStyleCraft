import { describe, expect, it } from "vitest";
import {
  DRAG_PX_FULL_RANGE,
  angleOf,
  arcPath,
  dragNorm,
  polar,
  wheelNorm,
} from "./knobMath";

describe("knob gesture ratios measured on FL Studio", () => {
  const range = DRAG_PX_FULL_RANGE;

  it("drags up to increase, clamped to 0..1", () => {
    expect(dragNorm(0.5, 300, 250, false)).toBeCloseTo(0.5 + 50 / range, 6);
    expect(dragNorm(0.5, 300, 350, false)).toBeLessThan(0.5);
    expect(dragNorm(0.95, 300, 100, false)).toBe(1);
    expect(dragNorm(0.05, 300, 500, false)).toBe(0);
  });

  it("Ctrl-drag is 0.276x of a normal drag (0.437 / 1.583)", () => {
    const normal = dragNorm(0.2, 300, 250, false) - 0.2;
    const fine = dragNorm(0.2, 300, 250, true) - 0.2;
    expect(fine / normal).toBeCloseTo(0.437 / 1.583, 2);
  });

  it("one wheel notch is 0.269x of a 50 px drag (0.426 / 1.583)", () => {
    const drag = dragNorm(0.2, 300, 250, false) - 0.2;
    const notch = wheelNorm(0.2, -100, false) - 0.2;
    expect(notch / drag).toBeCloseTo(0.426 / 1.583, 2);
    expect(wheelNorm(0.2, 100, false)).toBeLessThan(0.2);
  });
});

describe("knob geometry", () => {
  it("sweeps 270 degrees from 7:30 to 4:30", () => {
    expect(angleOf(0)).toBe(-135);
    expect(angleOf(0.5)).toBe(0);
    expect(angleOf(1)).toBe(135);
    expect(angleOf(2)).toBe(135);
  });

  it("places 0 degrees at 12 o'clock and 90 at 3 o'clock", () => {
    const top = polar(50, 50, 10, 0);
    expect(top.x).toBeCloseTo(50);
    expect(top.y).toBeCloseTo(40);
    const right = polar(50, 50, 10, 90);
    expect(right.x).toBeCloseTo(60);
    expect(right.y).toBeCloseTo(50);
  });

  it("builds no path for an empty arc and sets the large-arc flag past 180", () => {
    expect(arcPath(20, 20, 10, 10, 10)).toBe("");
    expect(arcPath(20, 20, 10, -135, 135)).toContain(" 1 1 ");
    expect(arcPath(20, 20, 10, 0, 90)).toContain(" 0 1 ");
  });
});
