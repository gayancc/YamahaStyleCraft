import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { UserVoiceEditor } from "./UserVoiceEditor";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
afterEach(cleanup);

it("releases a monitor that finishes opening after the editor closes", async () => {
  let opened!: () => void;
  invoke.mockReset().mockImplementation(async (command) =>
    command === "start_monitor" ? new Promise<void>((resolve) => { opened = resolve; }) : undefined,
  );
  const { unmount } = render(<UserVoiceEditor initial={null} midiInput="Keyboard" monitoringActive={false} onSave={() => {}} onCancel={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Learn from keyboard" }));
  unmount();
  expect(invoke.mock.calls.map(([command]) => command)).toEqual(["start_monitor"]);
  await act(async () => opened());
  expect(invoke.mock.calls.map(([command]) => command)).toEqual(["start_monitor", "stop_monitor"]);
});

it("learns a same-millisecond bank and program from one MIDI channel", async () => {
  const events = [
    [0xb2, 0, 104],
    [0xb2, 32, 21],
    [0xb4, 0, 127],
    [0xc2, 7],
  ].map((data) => ({ data, stampMs: 0, summary: "" }));
  invoke.mockImplementation(async (command) =>
    command === "monitor_snapshot" ? ["Keyboard", events] : undefined,
  );
  render(<UserVoiceEditor initial={null} midiInput="Keyboard" monitoringActive={false} onSave={() => {}} onCancel={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Learn from keyboard" }));
  await screen.findByText("Address learned from the keyboard.");
  expect(screen.getByLabelText("MSB")).toHaveProperty("value", "104");
  expect(screen.getByLabelText("LSB")).toHaveProperty("value", "21");
  expect(screen.getByLabelText("Program")).toHaveProperty("value", "8");
});
