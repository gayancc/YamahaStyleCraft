import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { App } from "./App";
import { useStudio } from "./store";
import type { View } from "./contracts";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
  confirm: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: () => false,
}));
vi.mock("@tauri-apps/plugin-dialog", () => mocks);
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));
const view: View = {
  name: "Actual contract test fixture",
  revision: 0,
  dirty: false,
  canUndo: false,
  canRedo: false,
  ppq: 1920,
  format: "SFF2",
  tempo: 131,
  numerator: 3,
  denominator: 4,
  sections: [{ id: 7, label: "Main A", start: 5760, end: 51840 }],
  notes: [
    {
      id: 1,
      offId: 2,
      track: 0,
      channel: 11,
      pitch: 36,
      velocity: 72,
      start: 5760,
      duration: 4436,
    },
  ],
  events: [],
  routes: [
    {
      id: 0,
      sections: ["Main A"],
      source: 11,
      destination: 11,
      sourceRoot: 0,
      sourceChord: 2,
      middleNtr: 1,
      middleNtt: 0,
      middleNttBass: false,
      bytes: Array.from({ length: 47 }, (_, i) =>
        i === 28 ? 1 : i === 19 ? 2 : 0,
      ),
    },
  ],
  chunks: [
    { tag: "MThd", bytes: 6 },
    { tag: "MTrk", bytes: 100 },
    { tag: "CASM", bytes: 2155 },
  ],
  warnings: [],
};
beforeEach(() => {
  vi.clearAllMocks();
  useStudio.setState({
    view: null,
    selected: [],
    channel: 11,
    busy: false,
    error: null,
    sectionId: 0,
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  mocks.confirm.mockResolvedValue(true);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
describe("real IPC wiring with isolated test mocks", () => {
  it("selects individual notes with keyboard-accessible inspector controls", () => {
    useStudio.setState({ view, sectionId: 7 });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Next note" }));
    expect(useStudio.getState().selected).toEqual([1]);
    expect(screen.getByRole("spinbutton", { name: "Velocity" })).toBeTruthy();
  });
  it("keeps keyboard focus inside MIDI settings", async () => {
    mocks.invoke.mockResolvedValue({ inputs: [], outputs: [] });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    const close = screen.getByRole("button", { name: "Close MIDI settings" });
    expect(document.activeElement).toBe(close);
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "Done" }),
    );
  });
  it("opens through the native dialog and displays numeric meter", async () => {
    mocks.open.mockResolvedValue("C:/test.STY");
    mocks.invoke.mockResolvedValue(view);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Open Yamaha Style" }));
    await screen.findByText("Actual contract test fixture");
    expect(mocks.invoke).toHaveBeenCalledWith("open_document", {
      path: "C:/test.STY",
    });
    expect(screen.getByText("3/4")).toBeTruthy();
  });
  it("commits a velocity edit using revision and note identity", async () => {
    useStudio.setState({ view, sectionId: 7, selected: [1] });
    mocks.invoke.mockResolvedValue({ ...view, revision: 1, dirty: true });
    render(<App />);
    const input = screen.getByRole("spinbutton", { name: "Velocity" });
    fireEvent.change(input, { target: { value: "73" } });
    fireEvent.blur(input);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("edit_document", {
        revision: 0,
        command: { type: "updateNotes", section: 7, ids: [1], velocity: 73 },
      }),
    );
  });
  it("does not replace current music after failed import", async () => {
    useStudio.setState({ view, sectionId: 7 });
    mocks.invoke.mockRejectedValue("Truncated CASM chunk");
    await useStudio.getState().load("broken.STY");
    expect(useStudio.getState().view).toBe(view);
    expect(useStudio.getState().error).toContain("Truncated CASM");
  });
  it("does not open another file after discard is declined", async () => {
    useStudio.setState({ view: { ...view, dirty: true }, sectionId: 7 });
    mocks.confirm.mockResolvedValue(false);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    await waitFor(() => expect(mocks.confirm).toHaveBeenCalled());
    expect(mocks.open).not.toHaveBeenCalled();
  });
  it("saves a self-contained project and reports native failures", async () => {
    useStudio.setState({ view, sectionId: 7 });
    mocks.save.mockResolvedValue("C:/edited.gss");
    mocks.invoke.mockRejectedValue("destination already exists");
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "Save project (Ctrl+S)" }),
    );
    await screen.findByRole("alert");
    expect(mocks.invoke).toHaveBeenCalledWith("save_document", {
      path: "C:/edited.gss",
      projectFile: true,
    });
    expect(screen.getByRole("alert").textContent).toContain(
      "destination already exists",
    );
  });
  it("requests devices without sending playback", async () => {
    mocks.invoke.mockResolvedValue({
      inputs: [],
      outputs: [{ index: 0, name: "Test output" }],
    });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    await screen.findByRole("dialog");
    await screen.findByRole("option", { name: "Test output" });
    expect(mocks.invoke).toHaveBeenCalledWith("scan_devices");
    expect(mocks.invoke).not.toHaveBeenCalledWith("play", expect.anything());
  });
  it("CASM root dispatch preserves route identity", async () => {
    useStudio.setState({ view, sectionId: 7 });
    mocks.invoke.mockResolvedValue({ ...view, revision: 1 });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "CASM" }));
    fireEvent.change(
      screen.getByRole("combobox", { name: "Route 0 source root" }),
      { target: { value: "2" } },
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("edit_document", {
        revision: 0,
        command: { type: "casm", route: 0, field: "root", value: 2 },
      }),
    );
  });
});
