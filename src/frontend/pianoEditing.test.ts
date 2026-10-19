import { describe, expect, it } from "vitest";
import type { Note, View } from "./contracts";
import {
  casmAdvisory,
  eqBandFromLane,
  eqLanePoints,
  eqValueBefore,
  EQ_HIGH_LANE,
  EQ_LOW_LANE,
  meterAt,
  moveBounds,
  nearestGridTick,
  quantizeStep,
  swungGridTick,
  tempoBpmFromMeta,
  tickFromElapsed,
  tickSnap,
  transformNotes,
  segmentHitsRect,
} from "./pianoEditing";
it("decodes SMF tempo meta to BPM", () => {
  // 500_000 µs/quarter = 120 BPM
  expect(tempoBpmFromMeta([0xff, 0x51, 3, 0x07, 0xa1, 0x20])).toBeCloseTo(120, 5);
  expect(tempoBpmFromMeta([0xb0, 7, 100])).toBeNull();
});
it("maps EQ virtual lanes and collapses NRPN packets to drawable points", () => {
  expect(eqBandFromLane(EQ_LOW_LANE)).toBe("eqLow");
  expect(eqBandFromLane(EQ_HIGH_LANE)).toBe("eqHigh");
  expect(eqBandFromLane(7)).toBeNull();
  const status = 0xba; // ch 11
  const packet = (tick: number, lsb: number, value: number, id0: number) => [
    { id: id0, tick, order: id0, data: [status, 99, 1] },
    { id: id0 + 1, tick, order: id0 + 1, data: [status, 98, lsb] },
    { id: id0 + 2, tick, order: id0 + 2, data: [status, 6, value] },
    { id: id0 + 3, tick, order: id0 + 3, data: [status, 101, 127] },
    { id: id0 + 4, tick, order: id0 + 4, data: [status, 100, 127] },
  ];
  const events = [
    ...packet(1920, 48, 80, 10),
    ...packet(2400, 49, 40, 20),
    { id: 30, tick: 1920, order: 30, data: [status, 7, 100] },
  ];
  expect(eqLanePoints(events, 1920, 5760, 11, "eqLow")).toEqual([
    { id: 12, tick: 1920, order: 12, data: [status, 6, 80] },
  ]);
  expect(eqLanePoints(events, 1920, 5760, 11, "eqHigh")).toEqual([
    { id: 22, tick: 2400, order: 22, data: [status, 6, 40] },
  ]);
  expect(eqValueBefore(events, 1920, 11, "eqLow")).toBeNull();
  expect(eqValueBefore(events, 2400, 11, "eqLow")).toBe(80);
});
it("erase strokes hit the line rather than unrelated notes inside its bounding rectangle", () => {
  expect(segmentHitsRect(0, 0, 100, 100, { x: 45, y: 45, w: 10, h: 10 })).toBe(
    true,
  );
  expect(segmentHitsRect(0, 0, 100, 100, { x: 5, y: 80, w: 10, h: 10 })).toBe(
    false,
  );
  expect(segmentHitsRect(10, 10, 10, 10, { x: 5, y: 5, w: 10, h: 10 })).toBe(
    true,
  );
  expect(segmentHitsRect(10, 10, 10, 10, { x: 20, y: 20, w: 10, h: 10 })).toBe(
    false,
  );
});
const section = { id: 7, label: "Main A", start: 1920, end: 5760 };
const note = (
  id: number,
  start: number,
  pitch = 60,
  duration = 120,
  velocity = 80,
): Note => ({
  id,
  offId: id + 100,
  track: 0,
  channel: 11,
  pitch,
  start,
  duration,
  velocity,
});
const ns = [
  note(1, 2041, 60, 240),
  note(2, 2281, 64, 240),
  note(3, 2521, 67, 240),
];
const patches = (
  kind: Parameters<typeof transformNotes>[0],
  options: Partial<Parameters<typeof transformNotes>[4]> = {},
) => {
  const cmd = transformNotes(kind, ns, section, 11, { step: 120, ...options });
  if (cmd.type !== "pianoNotes") throw Error("wrong contract");
  return cmd;
};
describe("independently authored MIDI note transforms", () => {
  it("quantizes relative to section with controlled strength and optional lengths", () => {
    const cmd = patches("quantize", { strength: 50 });
    expect(cmd.updates?.[0].start).toBe(2041); // a 1 tick error rounds at half strength
    expect(
      patches("quantize", { strength: 100 }).updates?.map((n) => n.start),
    ).toEqual([2040, 2280, 2520]);
    const out = transformNotes(
      "quantize",
      [note(1, section.end - 1, 60, 1)],
      section,
      11,
      { step: 480, quantizeDuration: true },
    );
    expect(out.type === "pianoNotes" && out.updates?.[0]).toMatchObject({
      start: section.end - 1,
      duration: 1,
    });
  });
  it("applies swing, tuplet, end targets, catch/safe, and rough timing", () => {
    expect(quantizeStep(120, 3)).toBe(160);
    expect(swungGridTick(1, 120, 60)).toBe(120 + Math.round((120 * 60) / 300));
    expect(nearestGridTick(130, 120, 60)).toBe(swungGridTick(1, 120, 60));
    const swung = patches("quantize", { swing: 60, strength: 100 });
    expect(swung.updates?.map((n) => n.start)).toEqual(
      [2041, 2281, 2521].map(
        (t) => section.start + nearestGridTick(t - section.start, 120, 60),
      ),
    );
    const trip = patches("quantize", { tuplet: 3, strength: 100 });
    expect(trip.updates?.[0].start).toBe(
      section.start + nearestGridTick(2041 - section.start, 160, 0),
    );
    const ends = transformNotes(
      "quantize",
      [note(1, 2040, 60, 241)],
      section,
      11,
      { step: 120, strength: 100, quantizeTarget: "ends" },
    );
    expect(ends.type === "pianoNotes" && ends.updates?.[0]).toMatchObject({
      start: 2040,
      duration: 240,
    });
    const both = transformNotes(
      "quantize",
      [note(1, 2041, 60, 251)],
      section,
      11,
      { step: 120, strength: 100, quantizeTarget: "both" },
    );
    expect(both.type === "pianoNotes" && both.updates?.[0]).toMatchObject({
      start: 2040,
      duration: 240,
    });
    const bothRough = transformNotes(
      "quantize",
      [note(1, 2040, 60, 240)],
      section,
      11,
      {
        step: 120,
        strength: 100,
        quantizeTarget: "both",
        roughTicks: 4,
        seed: 42,
      },
    );
    if (bothRough.type === "pianoNotes" && bothRough.updates?.[0]) {
      const u = bothRough.updates[0];
      expect(u.start + u.duration).toBe(2280);
      expect(Math.abs(u.start - 2040)).toBeLessThanOrEqual(4);
    }
    const caught = patches("quantize", {
      catchRange: 5,
      strength: 100,
    });
    // 2041 is 1 tick from 2040 → caught; 2281 is 1 from 2280 → caught
    expect(caught.updates?.map((n) => n.start)).toEqual([2040, 2280, 2520]);
    const missed = transformNotes(
      "quantize",
      [note(1, 2040 + 40, 60, 100)],
      section,
      11,
      { step: 120, strength: 100, catchRange: 10 },
    );
    expect(missed.type === "pianoNotes" && missed.updates?.[0].start).toBe(
      2080,
    );
    const safe = transformNotes(
      "quantize",
      [note(1, 2041, 60, 100)],
      section,
      11,
      { step: 120, strength: 100, safeRange: 5 },
    );
    expect(safe.type === "pianoNotes" && safe.updates?.[0].start).toBe(2041);
    const rough = transformNotes(
      "quantize",
      [note(1, 2040, 60, 100)],
      section,
      11,
      { step: 120, strength: 100, roughTicks: 4, seed: 42 },
    );
    const rStart =
      rough.type === "pianoNotes" ? rough.updates?.[0].start ?? 0 : 0;
    expect(Math.abs(rStart - 2040)).toBeLessThanOrEqual(4);
    expect(rStart).not.toBe(2040);
  });
  it("keeps unusual PPQ/triplet edits on integer ticks", () => {
    expect(tickSnap(109.3, (101 * 4) / 24)).toBe(101);
    const c = patches("quantize", { step: (101 * 4) / 24 });
    expect(
      c.updates?.every(
        (n) => Number.isInteger(n.start) && Number.isInteger(n.duration),
      ),
    ).toBe(true);
  });
  it("constrains group movement without collapsing intervals", () => {
    expect(
      moveBounds([note(1, 1920, 0), note(2, 5640, 127)], section, 500, -12),
    ).toEqual({ dt: 0, dp: 0 });
  });
  it("legato ends at the next distinct onset, bounded by section", () => {
    expect(patches("legato").updates?.map((n) => n.duration)).toEqual([
      240, 240, 3239,
    ]);
  });
  it("chop preserves total duration and properties with atomic replacement", () => {
    const c = patches("chop");
    expect(c.remove).toEqual([1, 2, 3]);
    expect(c.add).toHaveLength(6);
    expect(c.add?.reduce((s, n) => s + n.duration, 0)).toBe(720);
    expect(c.updates).toEqual([]);
  });
  it("glues touching same-pitch notes without merging different pitches/tracks", () => {
    const c = transformNotes(
      "glue",
      [note(1, 2040), note(2, 2160), note(3, 2280, 64)],
      section,
      11,
      { step: 120 },
    );
    expect(c.type === "pianoNotes" && c.remove).toEqual([2]);
    expect(c.type === "pianoNotes" && c.updates?.[0]).toMatchObject({
      id: 1,
      duration: 240,
    });
  });
  it("strums equal onsets by pitch order and leaves separate starts alone", () => {
    const c = transformNotes(
      "strum",
      [note(1, 2040, 67), note(2, 2040, 60), note(3, 2280, 64)],
      section,
      11,
      { step: 120, amount: 10 },
    );
    expect(c.type === "pianoNotes" && c.updates?.map((n) => n.start)).toEqual([
      2050, 2040, 2280,
    ]);
  });
  it("flam fails on section underflow rather than silently dropping attacks", () => {
    expect(() =>
      transformNotes("flam", [note(1, 1920)], section, 11, {
        step: 120,
        amount: 10,
      }),
    ).toThrow(/bounds/);
    expect(patches("flam", { amount: 10 }).add?.[0]).toMatchObject({
      start: 2031,
      duration: 10,
      velocity: 52,
    });
  });
  it("scales gate and timing while preserving integer ticks", () => {
    expect(patches("gate", { amount: 50 }).updates?.[0].duration).toBe(120);
    expect(patches("stretch", { amount: 200 }).updates?.[1]).toMatchObject({
      start: 2521,
      duration: 480,
    });
  });
  it("fitToBars stretches the selection span to an integer bar count", () => {
    const cmd = patches("fitToBars", { amount: 1, barTicks: 1920 });
    const starts = cmd.updates!.map((u) => u.start);
    const ends = cmd.updates!.map((u) => u.start + u.duration);
    expect(Math.max(...ends) - Math.min(...starts)).toBe(1920);
  });
  it("beatRepeat tiles the selection through the rest of the section", () => {
    const cmd = patches("beatRepeat", { amount: 480 });
    expect(cmd.add!.length).toBeGreaterThan(0);
    expect(cmd.updates).toHaveLength(3);
    expect(Math.max(...cmd.add!.map((n) => n.start))).toBeLessThan(section.end);
  });
  it("drumRoll inserts lead-in hits before each selected note", () => {
    const cmd = patches("drumRoll", { amount: 30 });
    expect(cmd.add!.length).toBeGreaterThan(0);
    expect(cmd.add!.every((n) => n.start >= section.start)).toBe(true);
    expect(Math.min(...cmd.add!.map((n) => n.start))).toBeLessThan(
      Math.min(...ns.map((n) => n.start)),
    );
  });
  it("flip is involutive for both pitch and time", () => {
    for (const kind of ["flipTime", "flipPitch"] as const) {
      const a = patches(kind);
      const moved = ns.map((n) => ({
        ...n,
        ...a.updates?.find((m) => m.id === n.id),
      }));
      const b = transformNotes(kind, moved, section, 11, { step: 120 });
      expect(b.type === "pianoNotes" && b.updates).toEqual(
        ns.map(({ id, start, duration, pitch, velocity }) => ({
          id,
          start,
          duration,
          pitch,
          velocity,
        })),
      );
    }
  });
  it("velocity scaling and randomization clamp to valid note-on range and seed deterministically", () => {
    expect(
      patches("velocityScale", { amount: 200 }).updates?.every(
        (n) => n.velocity === 127,
      ),
    ).toBe(true);
    expect(patches("velocityRandom", { amount: 20, seed: 42 })).toEqual(
      patches("velocityRandom", { amount: 20, seed: 42 }),
    );
    expect(
      patches("velocityRandom", { amount: 500, seed: 42 }).updates?.every(
        (n) => n.velocity >= 1 && n.velocity <= 127,
      ),
    ).toBe(true);
  });
  it("rejects empty selections and oversized chops", () => {
    expect(() => transformNotes("chop", [], section, 11, { step: 1 })).toThrow(
      /Select/,
    );
    expect(() =>
      transformNotes(
        "chop",
        [note(1, 0, 60, 100000)],
        { ...section, start: 0, end: 100001 },
        11,
        { step: 1 },
      ),
    ).toThrow(/50,000/);
  });
});
it("reads section meter and integrates tempo changes for playhead timing", () => {
  const view = {
    ppq: 480,
    tempo: 120,
    numerator: 4,
    denominator: 4,
    events: [
      { id: 1, order: 1, tick: 0, data: [255, 0x58, 4, 4, 2, 24, 8] },
      { id: 2, order: 2, tick: 1920, data: [255, 0x58, 4, 7, 3, 24, 8] },
      { id: 3, order: 3, tick: 2400, data: [255, 0x51, 3, 15, 66, 64] },
    ],
  } as View;
  expect(meterAt(view, 1920)).toMatchObject({
    numerator: 7,
    denominator: 8,
    beat: 240,
    bar: 1680,
  });
  expect(tickFromElapsed(view, section, 500)).toBe(2400);
  expect(tickFromElapsed(view, section, 1500)).toBe(2880);
});
it("CASM warning uses routed drum destinations without modifying source notes", () => {
  const view = {
    routes: [
      {
        source: 11,
        destination: 9,
        sections: ["Main A"],
        bytes: Array(47).fill(0),
        sourceRoot: 0,
        sourceChord: 2,
      },
    ],
  } as View;
  expect(casmAdvisory(view, section, 11, ns).drum).toBe(true);
});
