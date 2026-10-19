import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
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
  isTauri: false,
  onCloseRequested: vi.fn(),
  destroy: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: () => mocks.isTauri,
}));
vi.mock("@tauri-apps/plugin-dialog", () => mocks);
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: mocks.onCloseRequested,
    destroy: mocks.destroy,
  }),
}));
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
  mocks.onCloseRequested.mockResolvedValue(vi.fn());
  vi.clearAllMocks();
  mocks.invoke.mockReset().mockResolvedValue(null);
  mocks.isTauri = false;
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
it("hides and shows the inspector panel", () => {
  useStudio.setState({ view, sectionId: 7 });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Hide inspector" }));
  expect(screen.getByRole("button", { name: "Show inspector" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Hide inspector" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Show inspector" }));
  expect(screen.getByRole("button", { name: "Hide inspector" })).toBeTruthy();
});

it("parks the play cursor at section start and end", () => {
  useStudio.setState({ view, sectionId: 7 });
  render(<App />);
  fireEvent.click(
    screen.getByRole("button", { name: "Move cursor to section end (End)" }),
  );
  expect(useStudio.getState().message).toMatch(/section end/i);
  expect(
    screen.getByRole("button", { name: "Play from parked cursor (Space)" }),
  ).toBeTruthy();
  fireEvent.click(
    screen.getByRole("button", { name: "Move cursor to section start (W)" }),
  );
  expect(useStudio.getState().message).toMatch(/section start/i);
  expect(
    screen.getByRole("button", { name: "Play source notes (Space)" }),
  ).toBeTruthy();
});

it("selects Cursor and Pan piano-roll tools (SFM keys 2/3)", () => {
  useStudio.setState({ view, sectionId: 7, tool: "select" });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Park play cursor (2)" }));
  expect(useStudio.getState().tool).toBe("cursor");
  fireEvent.click(screen.getByRole("button", { name: "Pan piano roll (3)" }));
  expect(useStudio.getState().tool).toBe("pan");
});

it("claims Use MIDI here when another window owns hardware", async () => {
  mocks.isTauri = true;
  let owner = "project-2";
  mocks.invoke.mockImplementation(
    async (cmd: string, args?: { docLabel?: string }) => {
      if (cmd === "midi_ownership")
        return {
          owner,
          active: owner === (args?.docLabel ?? "main"),
          windows: 2,
        };
      if (cmd === "claim_midi") {
        owner = args?.docLabel ?? "main";
        return { owner, active: true, windows: 2 };
      }
      if (cmd === "load_prefs")
        return { schema: "genos.prefs.v1", snap: 16, zoom: 1, tool: "select" };
      if (cmd === "list_recent") return [];
      if (cmd === "recovery_status") return false;
      if (cmd === "scan_devices") return { inputs: [], outputs: [] };
      if (cmd === "transport_status")
        return { playing: false, elapsedMs: 0, error: null };
      return null;
    },
  );
  useStudio.setState({ view, sectionId: 7, docLabel: "main" });
  render(<App />);
  const claim = await screen.findByRole("button", { name: "Use MIDI here" });
  fireEvent.click(claim);
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "claim_midi",
      expect.objectContaining({ docLabel: "main" }),
    ),
  );
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "MIDI active here" }),
    ).toBeTruthy(),
  );
});

it("exposes Note colors pickers for all 16 channels", async () => {
  mocks.isTauri = true;
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        snap: 16,
        zoom: 1,
        tool: "select",
        noteColors: Array.from({ length: 16 }, () => "#112233"),
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "scan_devices") return { inputs: [], outputs: [] };
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    if (cmd === "save_prefs") return undefined;
    return null;
  });
  useStudio.setState({ view, sectionId: 7, channel: 11 });
  render(<App />);
  fireEvent.click(screen.getByText("Note colors"));
  const ch11 = await screen.findByLabelText("Note color CH11");
  expect(screen.getByLabelText("Note color CH1")).toBeTruthy();
  expect(screen.getByLabelText("Note color CH16")).toBeTruthy();
  await waitFor(() => expect((ch11 as HTMLInputElement).value).toBe("#112233"));
  fireEvent.input(ch11, { target: { value: "#abcdef" } });
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "save_prefs",
      expect.objectContaining({
        prefs: expect.objectContaining({
          noteColors: expect.arrayContaining(["#abcdef"]),
        }),
      }),
    ),
  );
});

it("mutes and solos source channels from the track list and filters play", async () => {
  useStudio.setState({ view, sectionId: 7 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return { inputs: [], outputs: [{ index: 1, name: "Genos" }] };
    if (cmd === "play" || cmd === "set_playback_channels") return undefined;
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("option", { name: "Genos" });
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "1:Genos" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Done" }));

  fireEvent.click(screen.getByRole("button", { name: "Mute CH11" }));
  expect(
    screen
      .getByRole("button", { name: "Mute CH11" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  fireEvent.click(screen.getByRole("button", { name: "Solo CH10" }));
  fireEvent.click(
    screen.getByRole("button", { name: "Play source notes (Space)" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "play",
      expect.objectContaining({ channels: [10] }),
    ),
  );

  // Live mute while playing pushes set_playback_channels (no plan restart).
  fireEvent.click(screen.getByRole("button", { name: "Mute CH10" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "set_playback_channels",
      expect.objectContaining({ channels: [], index: 1, name: "Genos" }),
    ),
  );

  fireEvent.click(
    screen.getByRole("button", { name: "Arm CH11 for recording" }),
  );
  expect(
    screen
      .getByRole("button", { name: "Arm CH11 for recording" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
}, 10_000);

it("requires arm and starts overdub play when capturing", async () => {
  mocks.isTauri = true;
  useStudio.setState({ view, sectionId: 7, channel: 10, error: null });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return {
        inputs: [{ index: 0, name: "Digital Workstation-1" }],
        outputs: [{ index: 1, name: "Genos" }],
      };
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: "Genos",
        midiOutputIndex: 1,
        snap: 16,
        zoom: 1,
        tool: "select",
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    if (cmd === "start_capture") return undefined;
    if (cmd === "play") return undefined;
    if (cmd === "capture_snapshot")
      return {
        recording: false,
        portName: null,
        events: [],
        markers: [],
        eventCount: 0,
        droppedEvents: 0,
      };
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("heading", { name: "MIDI capture" });
  await screen.findByRole("option", { name: "Digital Workstation-1" });
  fireEvent.change(screen.getByLabelText("MIDI input"), {
    target: { value: "Digital Workstation-1" },
  });
  await screen.findByRole("option", { name: "Genos" });
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "1:Genos" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Start capture" }));
  await waitFor(() =>
    expect(String(useStudio.getState().error ?? "")).toMatch(
      /Arm a part with R/i,
    ),
  );
  expect(mocks.invoke).not.toHaveBeenCalledWith(
    "start_capture",
    expect.anything(),
  );

  fireEvent.click(
    screen.getByRole("button", { name: "Arm CH11 for recording" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Start capture" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "start_capture",
      expect.objectContaining({ name: "Digital Workstation-1" }),
    ),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "play",
      expect.objectContaining({
        looping: true,
        channels: expect.arrayContaining([10, 11]),
      }),
    ),
  );
});

it("applies capture to the armed part channel", async () => {
  mocks.isTauri = true;
  useStudio.setState({ view, sectionId: 7, channel: 10 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return {
        inputs: [{ index: 0, name: "Digital Workstation-1" }],
        outputs: [],
      };
    if (cmd === "capture_snapshot")
      return {
        recording: false,
        portName: "Digital Workstation-1",
        events: [
          { stampMs: 10, data: [144, 60, 90], summary: "Note On" },
          { stampMs: 100, data: [128, 60, 0], summary: "Note Off" },
        ],
        markers: [],
        eventCount: 4,
        droppedEvents: 0,
      };
    if (cmd === "apply_capture_to_section") return { ...view, revision: 1 };
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: null,
        midiOutputIndex: null,
        snap: 16,
        zoom: 1,
        tool: "select",
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    return null;
  });
  render(<App />);
  fireEvent.click(
    screen.getByRole("button", { name: "Arm CH11 for recording" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("heading", { name: "MIDI capture" });
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Apply notes to section" }),
    ).not.toHaveProperty("disabled", true),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Apply notes to section" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "apply_capture_to_section",
      expect.objectContaining({
        section: 7,
        destinationChannel: 11,
        includeNotes: true,
        includeControllers: true,
        docLabel: "main",
      }),
    ),
  );
});

it("applies controllers-only capture when Record data is Controllers + Voice", async () => {
  mocks.isTauri = true;
  useStudio.setState({ view, sectionId: 7, channel: 10 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return {
        inputs: [{ index: 0, name: "Digital Workstation-1" }],
        outputs: [],
      };
    if (cmd === "capture_snapshot")
      return {
        recording: false,
        portName: "Digital Workstation-1",
        events: [
          { stampMs: 10, data: [0xb0, 11, 40], summary: "CC" },
          { stampMs: 20, data: [0xb0, 11, 90], summary: "CC" },
        ],
        markers: [],
        eventCount: 2,
        droppedEvents: 0,
      };
    if (cmd === "apply_capture_to_section") return { ...view, revision: 1 };
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: null,
        midiOutputIndex: null,
        snap: 16,
        zoom: 1,
        tool: "select",
        recData: "controllers",
        recInputChannel: 3,
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    return null;
  });
  render(<App />);
  fireEvent.click(
    screen.getByRole("button", { name: "Arm CH11 for recording" }),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Recording & MIDI settings" }),
  );
  await screen.findByRole("heading", { name: "Recording & MIDI" });
  await waitFor(() =>
    expect(screen.getByLabelText("Record data type")).toHaveProperty(
      "value",
      "controllers",
    ),
  );
  expect(screen.getByLabelText("Recording MIDI input channel")).toHaveProperty(
    "value",
    "3",
  );
  expect(
    screen.getByRole("button", { name: "Test metronome click" }),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("heading", { name: "MIDI capture" });
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Apply notes to section" }),
    ).not.toHaveProperty("disabled", true),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Apply notes to section" }),
  );
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "apply_capture_to_section",
      expect.objectContaining({
        includeNotes: false,
        includeControllers: true,
        destinationChannel: 11,
        inputChannel: 3,
        docLabel: "main",
      }),
    ),
  );
});

it("sends no initial effects by default, then includes the reviewed token only after explicit opt-in", async () => {
  useStudio.setState({ view, sectionId: 7 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return { inputs: [], outputs: [{ index: 1, name: "Genos" }] };
    if (cmd === "preview_style_effects")
      return {
        token: "9:0",
        packets: [[240, 247]],
        header: {
          reverb: { lateChanges: false },
          chorus: { lateChanges: false },
        },
      };
    return null;
  });
  render(<App />);
  const devices = async () => {
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    await screen.findByRole("option", { name: "Genos" });
  };
  const play = () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Play source notes (Space)" }),
    );
  await devices();
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "1:Genos" },
  });
  expect(
    (screen.getByLabelText("Preview initial Genos effects") as HTMLInputElement)
      .checked,
  ).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  play();
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "play",
      expect.objectContaining({ effectsToken: null }),
    ),
  );
  play();
  await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("stop"));
  await devices();
  fireEvent.click(screen.getByLabelText("Preview initial Genos effects"));
  await screen.findByText(/1 typed messages ready/);
  fireEvent.click(screen.getByRole("button", { name: "Done" }));
  play();
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "play",
      expect.objectContaining({
        section: 7,
        channels: [9, 10, 11, 12, 13, 14, 15, 16],
        index: 1,
        name: "Genos",
        looping: false,
        effectsToken: "9:0",
        fromTick: null,
        docLabel: "main",
      }),
    ),
  );
  play();
  await waitFor(() =>
    expect(mocks.invoke.mock.calls.filter(([c]) => c === "stop")).toHaveLength(
      2,
    ),
  );
  act(() => useStudio.setState({ view: { ...view, revision: 1 } }));
  await devices();
  expect(
    (screen.getByLabelText("Preview initial Genos effects") as HTMLInputElement)
      .checked,
  ).toBe(false);
  fireEvent.click(screen.getByLabelText("Preview initial Genos effects"));
  await screen.findByText(/typed messages ready/);
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "" },
  });
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "1:Genos" },
  });
  expect(
    (screen.getByLabelText("Preview initial Genos effects") as HTMLInputElement)
      .checked,
  ).toBe(false);
  // Raised from 5 000 ms: drives multiple play/stop cycles; runs more slowly
  // when the full suite loads extra modules (MixerPanel, DetachedMixer).
}, 12_000);
describe("real IPC wiring with isolated test mocks", () => {
  it("selects individual notes with keyboard-accessible inspector controls", () => {
    useStudio.setState({ view, sectionId: 7 });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Next note" }));
    expect(useStudio.getState().selected).toEqual([1]);
    expect(screen.getByRole("slider", { name: "Note velocity" })).toBeTruthy();
  });
  it("opens MIDI settings as a modeless desktop utility with device focus", async () => {
    mocks.invoke.mockResolvedValue({ inputs: [], outputs: [] });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    const frame = screen.getByRole("dialog", { name: "MIDI connections" });
    expect(frame.getAttribute("aria-modal")).toBe("false");
    expect(frame.parentElement).toBe(document.body);
    expect(document.activeElement).toBe(screen.getByLabelText("MIDI output"));
    expect(fireEvent.keyDown(frame, { key: "Tab", shiftKey: true })).toBe(true);
    fireEvent.keyDown(frame, { key: "Escape" });
    expect(
      screen.queryByRole("dialog", { name: "MIDI connections" }),
    ).toBeNull();
  });
  it("returns to MIDI connections after closing detailed recording settings", () => {
    mocks.invoke.mockResolvedValue({ inputs: [], outputs: [] });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Recording & MIDI settings…" }),
    );
    expect(
      screen.queryByRole("dialog", { name: "MIDI connections" }),
    ).toBeNull();
    expect(
      screen.getByRole("dialog", { name: "Recording & MIDI" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^Close$/ }));
    expect(
      screen.getByRole("dialog", { name: "MIDI connections" }),
    ).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByLabelText("MIDI output"));
  });
  it("opens through the native dialog and displays numeric meter", async () => {
    mocks.open.mockResolvedValue("C:/test.STY");
    mocks.invoke.mockResolvedValue(view);
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Open Yamaha Style" }));
    await screen.findByText("Actual contract test fixture");
    expect(mocks.invoke).toHaveBeenCalledWith("open_document", {
      path: "C:/test.STY",
      docLabel: "main",
    });
    expect(
      screen.getByRole("spinbutton", { name: "Meter numerator" }),
    ).toHaveProperty("value", "3");
    expect(
      screen.getByRole("combobox", { name: "Meter denominator" }),
    ).toHaveProperty("value", "4");
  });
  it("commits tempo and meter edits through setTempoMeter", async () => {
    useStudio.setState({ view, sectionId: 7 });
    mocks.invoke.mockResolvedValue({
      ...view,
      revision: 1,
      dirty: true,
      tempo: 100,
      numerator: 4,
      denominator: 4,
    });
    render(<App />);
    const tempo = screen.getByRole("spinbutton", { name: "Tempo BPM" });
    fireEvent.change(tempo, { target: { value: "100" } });
    fireEvent.blur(tempo);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("edit_document", {
        revision: 0,
        command: {
          type: "setTempoMeter",
          tempoBpm: 100,
          numerator: 3,
          denominator: 4,
        },
        docLabel: "main",
      }),
    );
  });
  it("exports section notes CSV through a noclobber save path", async () => {
    useStudio.setState({ view, sectionId: 7 });
    mocks.save.mockResolvedValue("C:/section-notes.csv");
    mocks.invoke.mockResolvedValue(undefined);
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "Export section notes CSV" }),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("export_section_csv", {
        path: "C:/section-notes.csv",
        section: 7,
      }),
    );
  });
  it("opens Help with known limits", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "Help" }));
    expect(screen.getByRole("heading", { name: "Help" })).toBeTruthy();
    expect(screen.getByText(/notes plus CC\/PC\/bend/i)).toBeTruthy();
    expect(screen.getByText(/hold right ~1s/i)).toBeTruthy();
  });
  it("opens feature inventory listing gated surfaces", () => {
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "Feature inventory / limits" }),
    );
    expect(
      screen.getByRole("heading", { name: "Feature inventory" }),
    ).toBeTruthy();
    expect(screen.getByText(/F027/i)).toBeTruthy();
    expect(screen.getByText(/EV-0171/i)).toBeTruthy();
  });
  it("opens MIDI connections with input monitor help tip", async () => {
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "scan_devices") {
        return {
          inputs: [{ index: 0, name: "Digital Workstation-1" }],
          outputs: [],
        };
      }
      return null;
    });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    await screen.findByRole("heading", { name: "MIDI connections" });
    expect(
      screen.getByRole("heading", { name: "MIDI input monitor" }),
    ).toBeTruthy();
    const tip = screen.getByRole("img", { name: "MIDI input monitor help" });
    expect(tip.getAttribute("data-tip") ?? "").toMatch(/transmit settings/i);
    expect(screen.getByLabelText("MIDI input")).toBeTruthy();
  });
  it("starts raw MIDI capture from device settings", async () => {
    useStudio.setState({ view, sectionId: 7 });
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "scan_devices") {
        return {
          inputs: [{ index: 0, name: "Digital Workstation-1" }],
          outputs: [{ index: 1, name: "Genos" }],
        };
      }
      if (cmd === "start_capture") return undefined;
      if (cmd === "play") return undefined;
      return null;
    });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    await screen.findByRole("heading", { name: "MIDI capture" });
    fireEvent.change(screen.getByLabelText("MIDI input"), {
      target: { value: "Digital Workstation-1" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Arm CH11 for recording" }),
    );
    fireEvent.click(screen.getByRole("button", { name: /Start capture/i }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        "start_capture",
        expect.objectContaining({
          name: "Digital Workstation-1",
          thruIndex: null,
          thruName: null,
          docLabel: "main",
        }),
      ),
    );
  });
  it("starts the display-only MIDI input monitor from device settings", async () => {
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "scan_devices") {
        return {
          inputs: [{ index: 0, name: "Digital Workstation-1" }],
          outputs: [],
        };
      }
      if (cmd === "start_monitor") return undefined;
      return null;
    });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "MIDI device settings" }),
    );
    await screen.findByRole("heading", { name: "MIDI input monitor" });
    fireEvent.change(screen.getByLabelText("MIDI input"), {
      target: { value: "Digital Workstation-1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Start monitor" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith(
        "start_monitor",
        expect.objectContaining({
          name: "Digital Workstation-1",
          docLabel: "main",
        }),
      ),
    );
  });
  it("creates a blank Style through Composer with a reference scaffold", async () => {
    mocks.open.mockResolvedValue("C:/reference.STY");
    mocks.invoke.mockResolvedValue({
      ...view,
      name: "My Yamaha Style",
      dirty: true,
      notes: [],
      tempo: 120,
      numerator: 4,
      denominator: 4,
    });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "New blank Style" }));
    await screen.findByRole("heading", { name: "Style Composer" });
    fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
    await waitFor(() =>
      expect(
        screen.getByRole("textbox", { name: "Reference Style path" }),
      ).toHaveProperty("value", "C:/reference.STY"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Create blank Style" }));
    await screen.findByText("My Yamaha Style");
    expect(mocks.invoke).toHaveBeenCalledWith("create_blank_document", {
      referencePath: "C:/reference.STY",
      options: {
        name: "My Yamaha Style",
        tempoBpm: 120,
        numerator: 4,
        denominator: 4,
      },
      docLabel: "main",
    });
    expect(screen.getByText("● Unsaved")).toBeTruthy();
  });
  it("rejects invalid Composer meter before calling native blank create", async () => {
    mocks.open.mockResolvedValue("C:/reference.STY");
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "New blank Style" }));
    await screen.findByRole("heading", { name: "Style Composer" });
    fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
    await waitFor(() =>
      expect(
        screen.getByRole("textbox", { name: "Reference Style path" }),
      ).toHaveProperty("value", "C:/reference.STY"),
    );
    const composer = screen.getByRole("dialog", { name: "Style Composer" });
    fireEvent.change(
      composer.querySelector(
        'input[aria-label="Meter numerator"]',
      ) as HTMLInputElement,
      { target: { value: "" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Create blank Style" }));
    expect((await screen.findByRole("alert")).textContent).toMatch(
      /numerator/i,
    );
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "create_blank_document",
      expect.anything(),
    );
  });
  it("commits a velocity edit using revision and note identity", async () => {
    useStudio.setState({ view, sectionId: 7, selected: [1] });
    mocks.invoke.mockResolvedValue({ ...view, revision: 1, dirty: true });
    render(<App />);
    fireEvent.click(
      screen.getByRole("button", { name: "Enter Note velocity value" }),
    );
    const input = screen.getByRole("spinbutton", {
      name: "Note velocity value",
    });
    fireEvent.change(input, { target: { value: "73" } });
    fireEvent.blur(input);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("edit_document", {
        revision: 0,
        command: { type: "updateNotes", section: 7, ids: [1], velocity: 73 },
        docLabel: "main",
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
      docLabel: "main",
      uiState: {
        sectionId: 7,
        channel: expect.any(Number),
        snap: expect.any(Number),
        zoom: expect.any(Number),
        tool: expect.any(String),
      },
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
        docLabel: "main",
      }),
    );
  });
  it("lists recent files on home and opens through open_document", async () => {
    mocks.isTauri = true;
    mocks.invoke.mockImplementation(
      async (cmd: string, args?: { path?: string; id?: string }) => {
        if (cmd === "list_recent") {
          return [
            {
              entry: {
                id: "abc123abc123abc123abc123",
                path: "C:/Styles/Demo.STY",
                name: "Demo.STY",
                kind: "sty",
                openedAt: "2026-10-16T12:00:00Z",
              },
              available: true,
            },
            {
              entry: {
                id: "def456def456def456def456",
                path: "C:/gone.gss",
                name: "gone.gss",
                kind: "project",
                openedAt: "2026-10-16T11:00:00Z",
              },
              available: false,
            },
          ];
        }
        if (cmd === "recovery_status") return false;
        if (cmd === "load_prefs") {
          return {
            schema: "genos.prefs.v1",
            midiOutputName: null,
            midiOutputIndex: null,
            snap: 16,
            zoom: 1,
            tool: "select",
          };
        }
        if (cmd === "open_document") return view;
        if (cmd === "transport_status") {
          return { playing: false, elapsedMs: 0, error: null };
        }
        if (cmd === "remove_recent") {
          expect(args?.id).toBe("def456def456def456def456");
          return [
            {
              entry: {
                id: "abc123abc123abc123abc123",
                path: "C:/Styles/Demo.STY",
                name: "Demo.STY",
                kind: "sty",
                openedAt: "2026-10-16T12:00:00Z",
              },
              available: true,
            },
          ];
        }
        return null;
      },
    );
    render(<App />);
    await screen.findByRole("region", { name: "Recent files" });
    expect(screen.getByText(/unavailable/i)).toBeTruthy();
    const goneOpen = screen
      .getAllByRole("button", { name: /gone\.gss/i })
      .find((b) => (b as HTMLButtonElement).disabled);
    expect(goneOpen).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Remove gone.gss from recent" }),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("remove_recent", {
        id: "def456def456def456def456",
      }),
    );
    const demoOpen = screen
      .getAllByRole("button", { name: /Demo\.STY/i })
      .find((b) => !b.getAttribute("aria-label")?.includes("Remove"));
    expect(demoOpen).toBeTruthy();
    fireEvent.click(demoOpen!);
    await screen.findByText("Actual contract test fixture");
    expect(mocks.invoke).toHaveBeenCalledWith("open_document", {
      path: "C:/Styles/Demo.STY",
      docLabel: "main",
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith("play", expect.anything());
  });
});

it("persists explicit output deselection instead of resurrecting the previous port", async () => {
  mocks.isTauri = true;
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: "Digital Workstation-1",
        midiOutputIndex: 1,
        snap: 16,
        zoom: 1,
        tool: "select",
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    if (cmd === "scan_devices")
      return {
        inputs: [],
        outputs: [{ index: 1, name: "Digital Workstation-1" }],
      };
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await waitFor(() =>
    expect(screen.getByLabelText("MIDI output")).toHaveProperty(
      "value",
      "1:Digital Workstation-1",
    ),
  );
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "" },
  });
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("save_prefs", {
      prefs: expect.objectContaining({
        midiOutputName: null,
        midiOutputIndex: null,
      }),
    }),
  );
  expect(mocks.invoke).not.toHaveBeenCalledWith("play", expect.anything());
});

it("blocks applying, exporting and clearing a take while recording", async () => {
  useStudio.setState({ view, sectionId: 7 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return {
        inputs: [{ index: 0, name: "Digital Workstation-1" }],
        outputs: [{ index: 1, name: "Genos" }],
      };
    if (cmd === "start_capture" || cmd === "play") return undefined;
    if (cmd === "capture_inject_cc")
      return { stampMs: 1, data: [0xba, 11, 40], summary: "CC" };
    if (cmd === "capture_snapshot")
      return {
        recording: true,
        portName: "Digital Workstation-1",
        events: [{ stampMs: 1, data: [0xba, 11, 40], summary: "CC" }],
        markers: [],
        eventCount: 1,
        droppedEvents: 0,
      };
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("heading", { name: "MIDI capture" });
  fireEvent.change(screen.getByLabelText("MIDI input"), {
    target: { value: "Digital Workstation-1" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Arm CH11 for recording" }),
  );
  fireEvent.click(screen.getByRole("button", { name: /Start capture/i }));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Stop capture" }),
    ).not.toHaveProperty("disabled", true),
  );
  for (const name of ["Cancel take", "Apply notes to section", "Export MIDI…"])
    expect(screen.getByRole("button", { name })).toHaveProperty(
      "disabled",
      true,
    );
  await waitFor(() =>
    expect(screen.getByLabelText("Controller to record")).toBeTruthy(),
  );
  fireEvent.change(screen.getByLabelText("Live controller value"), {
    target: { value: "40" },
  });
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith(
      "capture_inject_cc",
      expect.objectContaining({
        channel: 11,
        controller: 11,
        value: 40,
      }),
    ),
  );
});

it("closing a retained capture asks before discarding even without a dirty Style", async () => {
  mocks.isTauri = true;
  mocks.confirm.mockResolvedValue(false);
  useStudio.setState({ view, sectionId: 7 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: null,
        midiOutputIndex: null,
        snap: 16,
        zoom: 1,
        tool: "select",
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "scan_devices")
      return {
        inputs: [{ index: 0, name: "Digital Workstation-1" }],
        outputs: [{ index: 1, name: "Genos" }],
      };
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    if (cmd === "start_capture" || cmd === "play") return undefined;
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("heading", { name: "MIDI capture" });
  fireEvent.change(screen.getByLabelText("MIDI input"), {
    target: { value: "Digital Workstation-1" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Arm CH11 for recording" }),
  );
  fireEvent.click(screen.getByRole("button", { name: /Start capture/i }));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Stop capture" }),
    ).not.toHaveProperty("disabled", true),
  );
  const event = { preventDefault: vi.fn() };
  await mocks.onCloseRequested.mock.calls[0][0](event);
  expect(event.preventDefault).toHaveBeenCalled();
  expect(mocks.confirm).toHaveBeenCalled();
  expect(mocks.destroy).not.toHaveBeenCalled();
});

it("Cancel take confirms before discarding retained capture", async () => {
  mocks.isTauri = true;
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  useStudio.setState({ view, sectionId: 7 });
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_devices")
      return {
        inputs: [{ index: 0, name: "Digital Workstation-1" }],
        outputs: [],
      };
    if (cmd === "capture_snapshot")
      return {
        recording: false,
        portName: "Digital Workstation-1",
        events: [{ stampMs: 10, data: [0x90, 60, 80], summary: "Note" }],
        markers: [],
        eventCount: 1,
        droppedEvents: 0,
      };
    if (cmd === "clear_capture") return undefined;
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: null,
        midiOutputIndex: null,
        snap: 16,
        zoom: 1,
        tool: "select",
      };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await screen.findByRole("heading", { name: "MIDI capture" });
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Cancel take" }),
    ).not.toHaveProperty("disabled", true),
  );
  expect(screen.getByLabelText("MIDI IN inactive")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel take" }));
  expect(confirm).toHaveBeenCalled();
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("clear_capture"),
  );
  confirm.mockRestore();
});

it("Composer stays open while native creation is pending", async () => {
  mocks.open.mockResolvedValue("C:/reference.STY");
  let finish!: (v: View) => void;
  mocks.invoke.mockImplementation((cmd: string) =>
    cmd === "create_blank_document"
      ? new Promise<View>((resolve) => {
          finish = resolve;
        })
      : Promise.resolve(null),
  );
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "New blank Style" }));
  await screen.findByRole("heading", { name: "Style Composer" });
  fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
  await waitFor(() =>
    expect(screen.getByLabelText("Reference Style path")).toHaveProperty(
      "value",
      "C:/reference.STY",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "Create blank Style" }));
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Close Style Composer" }),
    ).toHaveProperty("disabled", true),
  );
  fireEvent.keyDown(screen.getByRole("dialog", { name: "Style Composer" }), {
    key: "Escape",
  });
  expect(screen.getByRole("dialog", { name: "Style Composer" })).toBeTruthy();
  finish({ ...view, notes: [], dirty: true });
  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "Style Composer" })).toBeNull(),
  );
});

it("rapid preference changes cannot restore a deselected MIDI output", async () => {
  mocks.isTauri = true;
  useStudio.setState({ view, sectionId: 7 });
  let prefs = {
    schema: "genos.prefs.v1",
    midiOutputName: "Digital Workstation-1" as string | null,
    midiOutputIndex: 1 as number | null,
    snap: 16,
    zoom: 1,
    tool: "select",
  };
  let writes = 0;
  mocks.invoke.mockImplementation(
    async (cmd: string, args?: { prefs: typeof prefs }) => {
      if (cmd === "load_prefs") {
        const snapshot = { ...prefs };
        await new Promise((resolve) => setTimeout(resolve, 20));
        return snapshot;
      }
      if (cmd === "save_prefs") {
        await new Promise((resolve) => setTimeout(resolve, 10));
        prefs = { ...args!.prefs };
        writes++;
        return null;
      }
      if (cmd === "scan_devices")
        return {
          inputs: [],
          outputs: [{ index: 1, name: "Digital Workstation-1" }],
        };
      if (cmd === "recovery_status") return false;
      if (cmd === "list_recent") return [];
      if (cmd === "transport_status")
        return { playing: false, elapsedMs: 0, error: null };
      return null;
    },
  );
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await waitFor(() =>
    expect(screen.getByLabelText("MIDI output")).toHaveProperty(
      "value",
      "1:Digital Workstation-1",
    ),
  );
  fireEvent.change(screen.getByLabelText("MIDI output"), {
    target: { value: "" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Close MIDI settings" }));
  fireEvent.click(screen.getByRole("button", { name: "Draw notes" }));
  await waitFor(() => expect(writes).toBe(2));
  expect(prefs.midiOutputName).toBeNull();
  expect(prefs.midiOutputIndex).toBeNull();
  expect(prefs.tool).toBe("draw");
});

it("reload hydrates a retained native take and keeps close protection", async () => {
  mocks.isTauri = true;
  mocks.confirm.mockResolvedValue(false);
  mocks.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "capture_snapshot")
      return {
        recording: false,
        portName: "Digital Workstation-1",
        events: [
          { stampMs: 0, data: [144, 60, 90], summary: "Note On" },
          { stampMs: 100, data: [128, 60, 0], summary: "Note Off" },
        ],
        markers: [],
        eventCount: 2,
        droppedEvents: 0,
      };
    if (cmd === "monitor_snapshot") return [null, []];
    if (cmd === "load_prefs")
      return {
        schema: "genos.prefs.v1",
        midiOutputName: null,
        midiOutputIndex: null,
        snap: 16,
        zoom: 1,
        tool: "select",
      };
    if (cmd === "scan_devices") return { inputs: [], outputs: [] };
    if (cmd === "list_recent") return [];
    if (cmd === "recovery_status") return false;
    if (cmd === "transport_status")
      return { playing: false, elapsedMs: 0, error: null };
    return null;
  });
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "MIDI device settings" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Export MIDI…" })).toHaveProperty(
      "disabled",
      false,
    ),
  );
  const event = { preventDefault: vi.fn() };
  await mocks.onCloseRequested.mock.calls[0][0](event);
  expect(event.preventDefault).toHaveBeenCalled();
  expect(mocks.confirm).toHaveBeenCalled();
  expect(mocks.destroy).not.toHaveBeenCalled();
});
