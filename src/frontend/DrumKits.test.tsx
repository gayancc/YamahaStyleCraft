import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DrumKits } from "./DrumKits";
import { useStudio } from "./store";
import type { Section, View } from "./contracts";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

const section: Section = {
  id: 1,
  label: "Main A",
  start: 0,
  end: 1920,
};

const view = {
  name: "Test",
  revision: 3,
  dirty: false,
  canUndo: false,
  canRedo: false,
  ppq: 480,
  format: "SFF2",
  tempo: 120,
  numerator: 4,
  denominator: 4,
  sections: [section],
  notes: [],
  events: [],
  routes: [],
  chunks: [],
  warnings: [],
} as View;

const kit = {
  category: "DrumKit",
  subcategory: "-",
  name: "RockDrumKit",
  msb: 127,
  lsb: 8,
  program: 17,
  voiceType: "Revo!Drums",
  page: 7,
};

describe("DrumKits", () => {
  it("does not let the kit dialog close a pending per-key edit on Escape", async () => {
    let finish!: (value: View) => void;
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd: string, args: unknown) =>
      cmd === "inspect_native_drums"
        ? Promise.resolve({
            kitName: "StandardKit",
            header: {
              channel: 9,
              setup: 1,
              address: { msb: 127, lsb: 0, program: 0 },
              parameterCount: 1,
              keys: [
                {
                  note: 36,
                  used: true,
                  volume: 77,
                  pan: null,
                  pitch: null,
                  fine: null,
                },
              ],
            },
          })
        : cmd === "apply_native_drums"
          ? new Promise((resolve) => {
              finish = resolve;
            })
          : previous(cmd, args),
    );
    const close = vi.fn();
    render(
      <DrumKits section={section} channel={9} output={null} onClose={close} />,
    );
    await screen.findByText("RockDrumKit");
    fireEvent.click(
      screen.getByRole("button", { name: "Genos per-key setup…" }),
    );
    await screen.findByLabelText("Drum Volume");
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "99" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Apply Drum Setup" }));
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(close).not.toHaveBeenCalled();
    finish({ ...view, revision: 4 });
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
  });
  afterEach(() => {
    cleanup();
  });
  beforeEach(() => {
    cleanup();
    useStudio.setState({ view, busy: false, error: null, message: "" });
    mocks.invoke.mockReset();
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "voice_catalog_status") {
        return {
          edition: "Genos v2.00",
          sourceName: "genos_en_dl_f0.pdf",
          sourceSha256: "abc",
          count: 1711,
        };
      }
      if (cmd === "drum_channel_status") {
        return [
          {
            channel: 9,
            address: { msb: 127, lsb: 0, program: 0 },
            voice: { ...kit, name: "StandardKit", program: 0 },
            lateProgram: false,
            kitCategory: true,
          },
          {
            channel: 10,
            address: null,
            voice: null,
            lateProgram: false,
            kitCategory: false,
          },
        ];
      }
      if (cmd === "search_drum_kits") {
        return { total: 1, voices: [kit] };
      }
      if (cmd === "apply_catalog_voice") {
        return { ...view, revision: 4 };
      }
      if (cmd === "preview_drum_note") return undefined;
      return null;
    });
  });

  it("lists DrumKit catalog rows and applies to channel 9–10", async () => {
    render(
      <DrumKits
        section={section}
        channel={9}
        output={{ index: 1, name: "Digital Workstation-1" }}
        onClose={vi.fn()}
      />,
    );
    await screen.findByText("RockDrumKit");
    expect(screen.getByLabelText("Rhythm channel kits").textContent).toMatch(
      /StandardKit/,
    );
    fireEvent.click(screen.getByText("RockDrumKit"));
    fireEvent.click(screen.getByRole("button", { name: /Apply kit to Ch/i }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        "apply_catalog_voice",
        expect.objectContaining({
          channel: 9,
          address: { msb: 127, lsb: 8, program: 17 },
        }),
      ),
    );
  });

  it("auditions a bounded drum note on the selected output", async () => {
    render(
      <DrumKits
        section={section}
        channel={10}
        output={{ index: 1, name: "Digital Workstation-1" }}
        onClose={vi.fn()}
      />,
    );
    await screen.findByRole("button", { name: "Audition note" });
    fireEvent.click(screen.getByRole("button", { name: "Audition note" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        "preview_drum_note",
        expect.objectContaining({
          index: 1,
          name: "Digital Workstation-1",
          channel: 10,
          pitch: 36,
          velocity: 90,
          holdMs: 200,
        }),
      ),
    );
  });

  it("rejects invalid notes and serializes a pending audition", async () => {
    render(
      <DrumKits
        section={section}
        channel={10}
        output={{ index: 1, name: "Genos" }}
        onClose={vi.fn()}
      />,
    );
    await screen.findByText("RockDrumKit");
    const input = screen.getByLabelText("Drum preview note");
    fireEvent.change(input, { target: { value: "92" } });
    fireEvent.click(screen.getByRole("button", { name: "Audition note" }));
    expect(
      await screen.findByText("Enter a drum note from 13 to 91."),
    ).toBeTruthy();
    expect(
      mocks.invoke.mock.calls.some(([cmd]) => cmd === "preview_drum_note"),
    ).toBe(false);
    fireEvent.change(input, { target: { value: "36" } });
    let finish!: () => void;
    const previous = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd: string, args: unknown) =>
      cmd === "preview_drum_note"
        ? new Promise<void>((resolve) => {
            finish = resolve;
          })
        : previous(cmd, args),
    );
    fireEvent.click(screen.getByRole("button", { name: "Audition note" }));
    fireEvent.click(screen.getByRole("button", { name: "Audition note" }));
    expect(
      mocks.invoke.mock.calls.filter(([cmd]) => cmd === "preview_drum_note"),
    ).toHaveLength(1);
    expect(
      (screen.getByRole("button", { name: "Close" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    finish();
    await waitFor(() =>
      expect(
        (screen.getByRole("button", { name: "Close" }) as HTMLButtonElement)
          .disabled,
      ).toBe(false),
    );
  });
});
