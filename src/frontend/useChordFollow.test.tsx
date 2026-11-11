import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useChordFollow } from "./useChordFollow";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

interface Keyboard {
  held: number[];
  all: number[];
  xf: {
    seq: number;
    root: number;
    quality: string;
    bass: number | null;
    bassQuality?: string;
  } | null;
}

function keyboard(initial: Partial<Keyboard> = {}) {
  const kb: Keyboard = { held: [], all: [], xf: null, ...initial };
  mocks.invoke.mockImplementation(
    async (cmd: string, args?: { maxPitch?: number | null }) => {
      if (cmd === "monitor_xf_chord") return kb.xf;
      if (cmd === "monitor_held_notes")
        return args?.maxPitch == null ? kb.all : kb.held;
      if (cmd === "recognize_chord_preview") {
        const notes = (args as unknown as { notes: number[] }).notes;
        return notes.join() === "48,52,55"
          ? { root: 0, quality: "Major" }
          : notes.join() === "50,53,57"
            ? { root: 2, quality: "Minor" }
            : null;
      }
      return undefined;
    },
  );
  return kb;
}

const chordCalls = () =>
  mocks.invoke.mock.calls
    .filter(([cmd, args]) => cmd === "set_live_chord" && args.root != null)
    .map(([, args]) => args);

const options = (over: Partial<Parameters<typeof useChordFollow>[0]> = {}) => ({
  enabled: true,
  inputName: "Genos",
  monitoring: true,
  setMonitoring: vi.fn(),
  docLabel: undefined,
  onError: vi.fn(),
  ...over,
});

describe("useChordFollow", () => {
  beforeEach(() => mocks.invoke.mockReset());
  afterEach(cleanup);

  it("tells the player the left-hand chord and follows each change", async () => {
    const kb = keyboard({ held: [48, 52, 55] });
    renderHook(() => useChordFollow(options({ docLabel: "project-2" })));
    await waitFor(() =>
      expect(chordCalls()).toEqual([{ root: 0, quality: "Major", bass: null, docLabel: "project-2" }]),
    );
    kb.held = [50, 53, 57];
    await waitFor(() => expect(chordCalls()).toHaveLength(2));
    expect(chordCalls()[1]).toEqual({ root: 2, quality: "Minor", bass: null, docLabel: "project-2" });
  });

  it("prefers the chord the keyboard's arranger reports", async () => {
    const kb = keyboard({ held: [48, 52, 55], xf: { seq: 1, root: 7, quality: "7", bass: 11 } });
    const { result } = renderHook(() => useChordFollow(options()));
    await waitFor(() =>
      expect(chordCalls()).toEqual([{ root: 7, quality: "7", bass: 11 }]),
    );
    expect(result.current).toContain("G 7");
    await waitFor(() =>
      expect(mocks.invoke.mock.calls.filter(([cmd]) => cmd === "monitor_xf_chord").length).toBeGreaterThan(1),
    );
    expect(mocks.invoke).not.toHaveBeenCalledWith("monitor_held_notes", expect.anything());
    kb.xf = { seq: 2, root: 7, quality: "7", bass: 11, bassQuality: "Minor" };
    await waitFor(() => expect(chordCalls()[1]).toEqual({
      root: 7, quality: "7", bass: 11, bassQuality: "Minor",
    }));
    kb.xf = { seq: 3, root: 7, quality: "Genos:58", bass: 11, bassQuality: "Genos:30" };
    await waitFor(() => expect(chordCalls()[2]).toEqual({
      root: 7, quality: "Genos:58", bass: 11, bassQuality: "Genos:30",
    }));
  });

  it("follows the latched Single Finger event result without a slash bass", async () => {
    keyboard();
    const respond = mocks.invoke.getMockImplementation()!;
    let chord = { root: 7, quality: "7" };
    mocks.invoke.mockImplementation(async (command, args) =>
      command === "monitor_single_finger_chord" ? chord : respond(command, args),
    );
    renderHook(() => useChordFollow(options({ singleFinger: true })));
    await waitFor(() => expect(chordCalls()).toEqual([{ root: 7, quality: "7", bass: null }]));
    chord = { root: 7, quality: "Major" };
    await waitFor(() => expect(chordCalls()[1]).toEqual({ root: 7, quality: "Major", bass: null }));
    expect(mocks.invoke).not.toHaveBeenCalledWith("monitor_held_notes", expect.anything());
  });

  it("retries a rejected keyboard chord without waiting for new notes", async () => {
    keyboard({ xf: { seq: 1, root: 7, quality: "7", bass: 11 } });
    const respond = mocks.invoke.getMockImplementation()!;
    let rejectFirst = true;
    mocks.invoke.mockImplementation(async (...args) => {
      if (args[0] === "set_live_chord" && rejectFirst) {
        rejectFirst = false;
        throw new Error("MIDI temporarily unavailable");
      }
      return respond(...args);
    });
    const onError = vi.fn();
    const { result } = renderHook(() => useChordFollow(options({ onError })));
    await waitFor(() => expect(onError).toHaveBeenCalledWith(expect.stringContaining("temporarily unavailable")));
    await waitFor(() => expect(result.current).toContain("G 7"));
    expect(chordCalls()).toHaveLength(2);
  });

  it("starts the input monitor itself and says what is missing", async () => {
    keyboard();
    const setMonitoring = vi.fn();
    renderHook(() =>
      useChordFollow(options({ monitoring: false, setMonitoring })),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("start_monitor", {
        name: "Genos",
        docLabel: undefined,
      }),
    );
    await waitFor(() => expect(setMonitoring).toHaveBeenCalledWith(true));
    const none = renderHook(() =>
      useChordFollow(options({ inputName: undefined, monitoring: false })),
    );
    expect(none.result.current).toMatch(/Select a MIDI input/);
  });

  it("explains notes played above the left-hand zone", async () => {
    keyboard({ all: [72, 76, 79] });
    const { result } = renderHook(() => useChordFollow(options()));
    await waitFor(() => expect(result.current).toMatch(/above B3/));
  });

  it("releases the chord when Live Play is turned off", async () => {
    keyboard();
    const { rerender } = renderHook((p) => useChordFollow(p), {
      initialProps: options(),
    });
    rerender(options({ enabled: false }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("set_live_chord", {
        root: null,
        quality: null,
        bass: null,
      }),
    );
  });
});
