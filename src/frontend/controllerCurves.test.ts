import { describe, expect, it } from "vitest";
import { curveT, isSpanCurve } from "./controllerCurves";

describe("controller curve shapes", () => {
  it("maps linear, ease, and step progress", () => {
    expect(curveT("ramp", 0.5)).toBe(0.5);
    expect(curveT("easeIn", 0.5)).toBe(0.25);
    expect(curveT("easeOut", 0.5)).toBe(0.75);
    expect(curveT("step", 0.5)).toBe(0);
    expect(curveT("step", 1)).toBe(1);
  });
  it("treats ramp and easing as span curves", () => {
    expect(isSpanCurve("ramp")).toBe(true);
    expect(isSpanCurve("easeIn")).toBe(true);
    expect(isSpanCurve("draw")).toBe(false);
  });
});
