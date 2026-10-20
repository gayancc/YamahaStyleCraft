import { describe, expect, it } from "vitest";
import { activityBins } from "./partActivity";

describe("activityBins", () => {
  it("is all zero without notes or a span", () => {
    expect(activityBins([], 0, 100, 4)).toEqual([0, 0, 0, 0]);
    expect(activityBins([{ start: 0, duration: 10 }], 5, 5, 4)).toEqual([0, 0, 0, 0]);
  });
  it("scales to the busiest slice and spreads long notes across slices", () => {
    const bins = activityBins(
      [
        { start: 0, duration: 10 },
        { start: 5, duration: 10 },
        { start: 50, duration: 50 },
      ],
      0,
      100,
      4,
    );
    expect(bins).toEqual([1, 0, 0.5, 0.5]);
  });
  it("ignores note starts outside the section", () => {
    expect(activityBins([{ start: -50, duration: 10 }, { start: 200, duration: 5 }], 0, 100, 2)).toEqual([0, 0]);
  });
});
