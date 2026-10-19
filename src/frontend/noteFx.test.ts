import { describe, expect, it } from "vitest";
import {
  buildNoteFxEnvelope,
  mergeNoteFxRanges,
  noteFxClearIds,
  previousControllerValue,
} from "./noteFx";

describe("noteFx", () => {
  it("merges overlapping notes into one span", () => {
    expect(
      mergeNoteFxRanges(
        [
          { id: 1, start: 100, duration: 50 },
          { id: 2, start: 120, duration: 80 },
          { id: 3, start: 300, duration: 40 },
        ],
        1000,
      ),
    ).toEqual([
      { start: 100, end: 200, noteIds: [1, 2] },
      { start: 300, end: 340, noteIds: [3] },
    ]);
  });

  it("builds linear envelope with restore at exclusive end", () => {
    const points = buildNoteFxEnvelope(
      { start: 0, end: 480, noteIds: [1] },
      {
        controller: 91,
        startValue: 0,
        endValue: 80,
        curve: "linear",
        restore: true,
      },
      [{ tick: 0, data: [0xba, 91, 10] }],
      480,
      1920,
    );
    expect(points[0]).toEqual({ tick: 0, value: 0 });
    expect(points.at(-1)).toEqual({ tick: 480, value: 10 });
    expect(points.length).toBeGreaterThanOrEqual(3);
  });

  it("uses binary sustain and step density of two", () => {
    const points = buildNoteFxEnvelope(
      { start: 100, end: 200, noteIds: [1] },
      {
        controller: 64,
        startValue: 0,
        endValue: 100,
        curve: "linear",
        restore: false,
      },
      [],
      480,
      1000,
    );
    expect(points).toEqual([
      { tick: 100, value: 0 },
      { tick: 199, value: 127 },
    ]);
  });

  it("keeps a one-tick note on lastTick with endValue", () => {
    const points = buildNoteFxEnvelope(
      { start: 100, end: 101, noteIds: [1] },
      {
        controller: 64,
        startValue: 0,
        endValue: 100,
        curve: "linear",
        restore: false,
      },
      [],
      480,
      1000,
    );
    expect(points).toEqual([{ tick: 100, value: 127 }]);
  });

  it("reads previous controller value before a tick", () => {
    expect(
      previousControllerValue(
        [
          { tick: 0, data: [0xba, 91, 20] },
          { tick: 50, data: [0xba, 91, 40] },
          { tick: 100, data: [0xba, 91, 60] },
        ],
        91,
        50,
      ),
    ).toBe(40);
  });

  it("clears [start,end) only unless restore tick is included", () => {
    const events = [
      { id: 1, tick: 100, data: [0xba, 91, 10] },
      { id: 2, tick: 150, data: [0xba, 91, 20] },
      { id: 3, tick: 200, data: [0xba, 91, 30] },
      { id: 4, tick: 150, data: [0xba, 7, 100] },
    ];
    const ranges = [{ start: 100, end: 200, noteIds: [9] }];
    expect(noteFxClearIds(events, 11, 91, ranges, 1000, false)).toEqual([
      1, 2,
    ]);
    expect(noteFxClearIds(events, 11, 91, ranges, 1000, true)).toEqual([
      1, 2, 3,
    ]);
  });
});
