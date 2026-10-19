import { beforeEach, expect, it, vi } from "vitest";
import { planClipboardPaste, isNoteClipboard } from "./noteClipboard";
import { useStudio } from "./store";
import type { Note, View } from "./contracts";
const mocks = vi.hoisted(() => ({ broadcast: vi.fn() }));
vi.mock("./detach", () => ({
  broadcastClipboardSync: mocks.broadcast,
  broadcastViewSync: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const a: Note = {
  id: 8,
  offId: 9,
  track: 2,
  channel: 11,
  start: 901,
  duration: 70,
  pitch: 60,
  velocity: 90,
};
const b: Note = {
  ...a,
  id: 10,
  offId: 11,
  start: 1021,
  duration: 72,
  pitch: 64,
  velocity: 110,
};
const destination = {
  ppq: 1920,
  section: { id: 1, label: "Main A", start: 1920, end: 10000 },
  cursor: 30,
  channel: 9,
  track: 0,
};
beforeEach(() => mocks.broadcast.mockClear());

it("scales musical spacing and duration while remapping to the destination source", () => {
  const source = { ppq: 480, notes: [a, b] };
  const before = JSON.stringify(source);
  expect(planClipboardPaste(source, destination).notes).toEqual([
    {
      start: 1950,
      duration: 280,
      pitch: 60,
      velocity: 90,
      track: 0,
      channel: 9,
    },
    {
      start: 2430,
      duration: 288,
      pitch: 64,
      velocity: 110,
      track: 0,
      channel: 9,
    },
  ]);
  expect(JSON.stringify(source)).toBe(before);
});

it("rounds relative onset independently of the absolute origin and keeps sub-tick notes playable", () => {
  const notes = [
    { ...a, duration: 1 },
    { ...b, start: 902, duration: 1 },
  ];
  const result = planClipboardPaste(
    { ppq: 480, notes },
    { ...destination, ppq: 240 },
  );
  expect(result.notes.map((n) => [n.start, n.duration])).toEqual([
    [1950, 1],
    [1951, 1],
  ]);
});

it("keeps same-clock timing exact, including unsorted input and simultaneous onsets", () => {
  const result = planClipboardPaste(
    { ppq: 480, notes: [b, a, { ...a, pitch: 67 }] },
    { ...destination, ppq: 480 },
  );
  expect(result.notes.map((n) => [n.start, n.duration, n.pitch])).toEqual([
    [2070, 72, 64],
    [1950, 70, 60],
    [1950, 70, 67],
  ]);
});

it("rejects missing/invalid source clock and malformed notes instead of guessing timing", () => {
  for (const ppq of [undefined, null, 0, -1, 32768, 1.5, NaN]) {
    expect(isNoteClipboard({ ppq, notes: [a] })).toBe(false);
  }
  for (const patch of [
    { start: NaN },
    { duration: 0 },
    { velocity: 0 },
    { pitch: 128 },
    { start: 1_000_000_000 },
    { id: Number.MAX_SAFE_INTEGER + 1 },
  ]) {
    expect(isNoteClipboard({ ppq: 480, notes: [{ ...a, ...patch }] })).toBe(
      false,
    );
  }
  expect(() => planClipboardPaste({ ppq: 0, notes: [a] }, destination)).toThrow(
    /clipboard timing/,
  );
  expect(() =>
    planClipboardPaste({ ppq: 480, notes: [a] }, { ...destination, ppq: 0 }),
  ).toThrow(/clipboard timing/);
});

it("shifts a fitting phrase left together without snapping at the section edge", () => {
  const plan = planClipboardPaste(
    { ppq: 480, notes: [a, b] },
    { ...destination, cursor: 8000 },
  );
  expect(plan.shifted).toBe(true);
  expect(plan.omitted).toBe(0);
  expect(plan.notes.map((n) => [n.start, n.duration])).toEqual([
    [9232, 280],
    [9712, 288],
  ]);
});

it("clips and reports oversized phrases without shifting notes beyond the end backward", () => {
  const source = {
    ppq: 480,
    notes: [
      { ...a, duration: 3000 },
      { ...b, start: 5000 },
    ],
  };
  const plan = planClipboardPaste(source, { ...destination, cursor: 7000 });
  expect(plan.shifted).toBe(false);
  expect(plan.omitted).toBe(1);
  expect(plan.clipped).toBe(1);
  expect(plan.notes.map((n) => [n.start, n.duration])).toEqual([[8920, 1080]]);
});

it("copies and broadcasts a timing snapshot that survives a project switch", () => {
  useStudio.setState({
    view: { ppq: 480 } as View,
    clipboard: [],
    clipboardPpq: null,
  });
  const original = { ...a };
  useStudio.getState().setClipboard([original]);
  original.duration = 999;
  useStudio.setState({ view: { ppq: 1920 } as View });
  const state = useStudio.getState();
  expect(state.clipboardPpq).toBe(480);
  expect(state.clipboard[0].duration).toBe(70);
  expect(mocks.broadcast).toHaveBeenCalledWith({ ppq: 480, notes: [a] });
  expect(
    planClipboardPaste(
      { ppq: state.clipboardPpq!, notes: state.clipboard },
      destination,
    ).notes[0].duration,
  ).toBe(280);
});

it("keeps the existing clipboard when Copy has no selection and reports failed writes", async () => {
  useStudio.setState({
    view: { ppq: 480 } as View,
    clipboard: [a],
    clipboardPpq: 480,
    error: null,
  });
  expect(await useStudio.getState().setClipboard([])).toBe(false);
  expect(useStudio.getState().clipboard).toEqual([a]);
  mocks.broadcast.mockRejectedValueOnce(Error("write unavailable"));
  expect(await useStudio.getState().setClipboard([b])).toBe(false);
  expect(useStudio.getState().error).toContain("Copy failed");
  expect(useStudio.getState().clipboard).toEqual([a]);
});
