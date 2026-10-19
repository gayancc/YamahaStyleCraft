import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NativeDrumSetup, type NativeDrumContext } from "./NativeDrumSetup";
import { useStudio } from "./store";
import type { View } from "./contracts";
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
const view = {
  name: "Drums",
  revision: 3,
  notes: [],
  routes: [],
  sections: [],
  events: [],
  chunks: [],
  warnings: [],
} as unknown as View;
const context: NativeDrumContext = {
  kitName: "StandardKit",
  header: {
    channel: 9,
    setup: 1,
    address: { msb: 127, lsb: 0, program: 0 },
    parameterCount: 2,
    keys: [
      { note: 36, used: true, volume: 77, pan: null, pitch: null, fine: 0 },
      { note: 38, used: false, volume: null, pan: 0, pitch: 0, fine: 3 },
    ],
  },
};
describe("NativeDrumSetup", () => {
  beforeEach(() => {
    useStudio.setState({ view, busy: false });
    mocks.invoke.mockReset();
    mocks.invoke.mockImplementation(async (cmd) =>
      cmd === "inspect_native_drums" ? context : { ...view, revision: 4 },
    );
  });
  afterEach(() => cleanup());
  it("auditions the selected drum key on the MIDI output", async () => {
    render(
      <NativeDrumSetup
        channel={9}
        output={{ index: 1, name: "Genos" }}
        onClose={vi.fn()}
      />,
    );
    await screen.findByText(/StandardKit/);
    fireEvent.click(screen.getByRole("button", { name: "Audition drum key" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("preview_drum_note", {
        index: 1,
        name: "Genos",
        channel: 9,
        pitch: 36,
        velocity: 90,
        holdMs: 200,
      }),
    );
  });
  it("confirms a multi-key scalar batch with raw pan and semitone values in one transaction", async () => {
    const close = vi.fn();
    render(<NativeDrumSetup channel={9} onClose={close} />);
    await screen.findByText(/StandardKit/);
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "100" },
    });
    fireEvent.change(screen.getByLabelText("Drum Pan"), {
      target: { value: "64" },
    });
    fireEvent.change(screen.getByLabelText("Drum Pitch (semitones)"), {
      target: { value: "-2" },
    });
    fireEvent.change(screen.getByLabelText("Native drum key"), {
      target: { value: "38" },
    });
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "90" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Apply Drum Setup" }));
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(mocks.invoke).toHaveBeenCalledWith("apply_native_drums", {
      channel: 9,
      revision: 3,
      address: { msb: 127, lsb: 0, program: 0 },
      changes: [
        { note: 36, field: "volume", value: 100 },
        { note: 36, field: "pan", value: 64 },
        { note: 36, field: "pitch", value: -2 },
        { note: 38, field: "volume", value: 90 },
      ],
    });
    expect(useStudio.getState().view?.revision).toBe(4);
    expect(useStudio.getState().busy).toBe(false);
  });
  it("keeps defaults unknown, omits blanks and exact noops, and cancel sends nothing", async () => {
    const close = vi.fn();
    render(<NativeDrumSetup channel={9} onClose={close} />);
    await screen.findByText(/StandardKit/);
    expect(
      (screen.getByLabelText("Drum Pan") as HTMLInputElement).placeholder,
    ).toBe("Kit default");
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "77" },
    });
    expect(
      (
        screen.getByRole("button", {
          name: "Apply Drum Setup",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(close).toHaveBeenCalledOnce();
    expect(mocks.invoke.mock.calls).toHaveLength(1);
  });
  it("rejects out-of-range values and locks non-neutral fine tuning", async () => {
    render(<NativeDrumSetup channel={9} onClose={vi.fn()} />);
    await screen.findByText(/StandardKit/);
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "128" },
    });
    expect(
      (
        screen.getByRole("button", {
          name: "Apply Drum Setup",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.change(screen.getByLabelText("Native drum key"), {
      target: { value: "38" },
    });
    expect(
      (screen.getByLabelText("Drum Pitch (semitones)") as HTMLInputElement)
        .disabled,
    ).toBe(true);
    expect(screen.getByText(/non-neutral fine tuning/)).toBeTruthy();
  });
  it("shows unsupported headers without enabling edits", async () => {
    mocks.invoke.mockRejectedValue("Several channels share this drum setup");
    render(<NativeDrumSetup channel={9} onClose={vi.fn()} />);
    await screen.findByRole("alert");
    expect(screen.getByRole("alert").textContent).toContain("share");
    expect(
      (
        screen.getByRole("button", {
          name: "Apply Drum Setup",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });
  it("rejects replaced documents even if their revision matches", async () => {
    render(<NativeDrumSetup channel={9} onClose={vi.fn()} />);
    await screen.findByText(/StandardKit/);
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "100" },
    });
    act(() => useStudio.setState({ view: { ...view, name: "Another" } }));
    expect(
      (
        screen.getByRole("button", {
          name: "Apply Drum Setup",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(mocks.invoke.mock.calls).toHaveLength(1);
  });
  it("holds close and duplicate apply while pending and retains changes after an error", async () => {
    const close = vi.fn();
    let fail!: (reason: string) => void;
    mocks.invoke.mockImplementation((cmd) =>
      cmd === "inspect_native_drums"
        ? Promise.resolve(context)
        : new Promise((_, reject) => {
            fail = reject;
          }),
    );
    render(<NativeDrumSetup channel={9} onClose={close} />);
    await screen.findByText(/StandardKit/);
    fireEvent.change(screen.getByLabelText("Drum Volume"), {
      target: { value: "99" },
    });
    const apply = screen.getByRole("button", { name: "Apply Drum Setup" });
    fireEvent.click(apply);
    fireEvent.click(apply);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(close).not.toHaveBeenCalled();
    expect(
      mocks.invoke.mock.calls.filter(([cmd]) => cmd === "apply_native_drums"),
    ).toHaveLength(1);
    await act(async () => fail("Disk error"));
    await screen.findByText("Disk error");
    expect(
      (screen.getByLabelText("Drum Volume") as HTMLInputElement).value,
    ).toBe("99");
    expect(useStudio.getState().busy).toBe(false);
  });
});
