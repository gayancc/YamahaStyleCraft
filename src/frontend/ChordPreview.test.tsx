import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ChordPreview } from "./ChordPreview";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./store", () => {
  const state = {
    view: {
      revision: 1,
      notes: [
        {
          id: 1,
          offId: 2,
          track: 0,
          channel: 11,
          pitch: 60,
          start: 0,
          duration: 100,
          velocity: 80,
        },
        {
          id: 3,
          offId: 4,
          track: 0,
          channel: 11,
          pitch: 64,
          start: 0,
          duration: 100,
          velocity: 80,
        },
        {
          id: 5,
          offId: 6,
          track: 0,
          channel: 11,
          pitch: 67,
          start: 0,
          duration: 100,
          velocity: 80,
        },
      ],
    },
  };
  return { useStudio: () => state };
});

describe("ChordPreview", () => {
  afterEach(() => cleanup());
  beforeEach(() => {
    cleanup();
    mocks.invoke.mockReset();
    mocks.invoke.mockImplementation(
      async (cmd: string, args?: { quality?: string }) => {
        if (cmd === "chord_preview_vocabulary") {
          return {
            roots: ["C", "C#", "D"],
            qualities: ["Major", "Minor"],
            disclaimer: "test",
          };
        }
        if (cmd === "inspect_chord_preview") {
          return {
            root: 0,
            rootName: "C",
            quality: args?.quality ?? "Major",
            alias: null,
            tones: [0, 4, 7],
            scale: [0, 2, 4, 5, 7, 9, 11],
            absoluteTones: [0, 4, 7],
            disclaimer: "not yamaha",
          };
        }
        if (cmd === "remap_chord_preview_notes") {
          return [
            { source: 60, preview: 60, silent: false },
            { source: 64, preview: 64, silent: false },
            { source: 67, preview: 67, silent: false },
          ];
        }
        if (cmd === "pitch_shift_bend_message") {
          return { status: 0xe0, data1: 0, data2: 0x40, value14: 8192 };
        }
        if (cmd === "pitch_shift_wire_pitch") {
          return 58;
        }
        if (cmd === "pitch_shift_retarget") {
          return {
            messages: [{ status: 0xe0, data1: 0, data2: 0x50 }],
            wirePitches: [60],
            shifted: true,
            fallback: false,
          };
        }
        return null;
      },
    );
  });

  it("lists tones and remapped source pitches", async () => {
    render(
      <ChordPreview
        section={{ id: 1, label: "Main A", start: 0, end: 1000 } as never}
        channel={11}
        onClose={vi.fn()}
      />,
    );
    await screen.findByText(/C Major/i);
    expect(screen.getByText(/Not a Yamaha arranger emulator/i)).toBeTruthy();
    expect(screen.getByLabelText("Preview kind")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Source chord root"), {
      target: { value: "2" },
    });
    fireEvent.change(screen.getByLabelText("Source chord quality"), {
      target: { value: "Minor" },
    });
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        "remap_chord_preview_notes",
        expect.objectContaining({ sourceRoot: 2, sourceQuality: "Minor" }),
      ),
    );
    fireEvent.change(screen.getByLabelText("Preview chord quality"), {
      target: { value: "Minor" },
    });
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        "inspect_chord_preview",
        expect.objectContaining({ quality: "Minor" }),
      ),
    );
  });
});
