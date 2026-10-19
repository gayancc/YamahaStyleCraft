import { beforeEach, afterEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { ControllerLane } from "./ControllerLane";
import { useStudio } from "./store";
import type { Note, View } from "./contracts";
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => mock);
const section = { id: 7, label: "Main A", start: 1920, end: 5760 };
const notes: Note[] = [
  {
    id: 1,
    offId: 2,
    track: 0,
    channel: 11,
    pitch: 60,
    start: 1920,
    duration: 120,
    velocity: 80,
  },
  {
    id: 3,
    offId: 4,
    track: 0,
    channel: 11,
    pitch: 64,
    start: 2400,
    duration: 120,
    velocity: 60,
  },
  {
    id: 5,
    offId: 6,
    track: 0,
    channel: 11,
    pitch: 67,
    start: 1920,
    duration: 120,
    velocity: 20,
  },
];
const view = {
  name: "lane",
  revision: 0,
  ppq: 480,
  tempo: 120,
  numerator: 4,
  denominator: 4,
  notes,
  sections: [section],
  events: [
    { id: 10, tick: 1920, order: 10, data: [0xba, 7, 50] },
    { id: 11, tick: 2400, order: 11, data: [0xba, 7, 90] },
  ],
  routes: [],
  chunks: [],
  warnings: [],
  canUndo: false,
  canRedo: false,
  dirty: false,
  format: "SFF2",
} satisfies View;
const canvasProto = HTMLCanvasElement.prototype as HTMLCanvasElement & {
  setPointerCapture: (id: number) => void;
  hasPointerCapture: (id: number) => boolean;
  releasePointerCapture: (id: number) => void;
};
const origPointerCapture = {
  set: Object.getOwnPropertyDescriptor(canvasProto, "setPointerCapture"),
  has: Object.getOwnPropertyDescriptor(canvasProto, "hasPointerCapture"),
  release: Object.getOwnPropertyDescriptor(canvasProto, "releasePointerCapture"),
};
beforeEach(() => {
  useStudio.setState({
    view,
    sectionId: 7,
    channel: 11,
    selected: [],
    busy: false,
    error: null,
  });
  mock.invoke.mockReset().mockResolvedValue({ ...view, revision: 1 });
  vi.stubGlobal(
    "PointerEvent",
    class extends MouseEvent {
      pointerId = 1;
    },
  );
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  Object.defineProperty(canvasProto, "setPointerCapture", {
    configurable: true,
    writable: true,
    value: vi.fn(),
  });
  Object.defineProperty(canvasProto, "hasPointerCapture", {
    configurable: true,
    writable: true,
    value: vi.fn(() => true),
  });
  Object.defineProperty(canvasProto, "releasePointerCapture", {
    configurable: true,
    writable: true,
    value: vi.fn(),
  });
});
afterEach(() => {
  for (const [key, desc] of [
    ["setPointerCapture", origPointerCapture.set],
    ["hasPointerCapture", origPointerCapture.has],
    ["releasePointerCapture", origPointerCapture.release],
  ] as const) {
    if (desc) Object.defineProperty(canvasProto, key, desc);
    else delete (canvasProto as unknown as Record<string, unknown>)[key];
  }
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
const mount = (v = view) =>
  render(
    <ControllerLane
      notes={v.notes}
      section={section}
      view={v}
      channel={11}
      width={800}
      scale={0.2}
      offset={0}
      grid={120}
      disabled={false}
      playheadTick={null}
    />,
  );
const gesture = (
  c: HTMLElement,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
) => {
  fireEvent.pointerDown(c, { clientX: x1, clientY: y1, button: 0 });
  fireEvent.pointerMove(c, { clientX: x2, clientY: y2, button: 0 });
};
const command = () => mock.invoke.mock.calls[0]?.[1]?.command;
it("previews continuously then sends exactly one scoped velocity command", async () => {
  mount();
  const c = screen.getByLabelText(/^Velocity lane[.]/);
  gesture(c, 60, 48, 60, 90);
  expect(mock.invoke).not.toHaveBeenCalled();
  fireEvent.pointerUp(c);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command()).toMatchObject({
    type: "pianoNotes",
    section: 7,
    channel: 11,
    updates: [{ id: 1, velocity: 20 }],
  });
});
it("overlapping handles select by vertical distance rather than array order", async () => {
  mount();
  const c = screen.getByLabelText(/^Velocity lane[.]/);
  gesture(c, 60, 90, 60, 60);
  fireEvent.pointerUp(c);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command().updates).toHaveLength(1);
  expect(command().updates[0].id).toBe(5);
});
it("cancelled velocity drafts never invoke the backend", () => {
  mount();
  const c = screen.getByLabelText(/^Velocity lane[.]/);
  gesture(c, 60, 48, 100, 30);
  fireEvent.pointerCancel(c);
  fireEvent.pointerUp(c);
  expect(mock.invoke).not.toHaveBeenCalled();
});
it("selection restricts a velocity ramp to selected note identities", async () => {
  useStudio.setState({ selected: [1, 3] });
  mount();
  fireEvent.change(screen.getByLabelText("Controller gesture"), {
    target: { value: "ramp" },
  });
  const c = screen.getByLabelText(/^Velocity lane[.]/);
  gesture(c, 60, 90, 156, 16);
  fireEvent.pointerUp(c);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalled());
  expect(command().updates.map((n: { id: number }) => n.id)).toEqual([1, 3]);
});
it("Pitch bend lane commits 14-bit pitchBendPoints", async () => {
  mount();
  fireEvent.change(screen.getByLabelText("Controller lane"), {
    target: { value: "-4" },
  });
  fireEvent.change(screen.getByLabelText("Controller gesture"), {
    target: { value: "ramp" },
  });
  const c = screen.getByLabelText(/^Pitch bend lane/);
  gesture(c, 60, 104, 156, 16);
  fireEvent.pointerUp(c);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command()).toMatchObject({
    type: "pitchBendPoints",
    section: 7,
    channel: 11,
  });
  expect(command().points[0]).toEqual({ tick: 1920, value: 0 });
  expect(command().points.at(-1)).toEqual({ tick: 2400, value: 16383 });
});
it("EQ Low lane commits timed NRPN automation via mixerEqPoints", async () => {
  mount();
  fireEvent.change(screen.getByLabelText("Controller lane"), {
    target: { value: "-2" },
  });
  fireEvent.change(screen.getByLabelText("Controller gesture"), {
    target: { value: "ramp" },
  });
  const c = screen.getByLabelText(/^EQ Low NRPN lane/);
  gesture(c, 60, 104, 156, 16);
  fireEvent.pointerUp(c);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command()).toMatchObject({
    type: "mixerEqPoints",
    section: 7,
    channel: 11,
    band: "eqLow",
  });
  expect(command().points[0]).toEqual({ tick: 1920, value: 0 });
  expect(command().points.at(-1)).toEqual({ tick: 2400, value: 127 });
});
it("CC7 creates explicit 0–127 samples and retains intermediate existing event ticks", async () => {
  mount();
  fireEvent.change(screen.getByLabelText("Controller lane"), {
    target: { value: "7" },
  });
  fireEvent.change(screen.getByLabelText("Controller gesture"), {
    target: { value: "ramp" },
  });
  const c = screen.getByLabelText(/^MIDI CC7 lane[.]/);
  gesture(c, 60, 104, 156, 16);
  fireEvent.pointerUp(c);
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command()).toMatchObject({
    type: "controllerPoints",
    section: 7,
    channel: 11,
    controller: 7,
  });
  expect(command().points[0]).toEqual({ tick: 1920, value: 0 });
  expect(command().points.at(-1)).toEqual({ tick: 2400, value: 127 });
  expect(command().points).toHaveLength(5);
});
it("controller selection supports select-all deletion without deleting notes", async () => {
  mount();
  fireEvent.change(screen.getByLabelText("Controller lane"), {
    target: { value: "7" },
  });
  fireEvent.change(screen.getByLabelText("Controller gesture"), {
    target: { value: "relative" },
  });
  const c = screen.getByLabelText(/^MIDI CC7 lane[.]/);
  fireEvent.keyDown(c, { key: "a", ctrlKey: true });
  fireEvent.keyDown(c, { key: "Delete" });
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command()).toEqual({
    type: "controllerPoints",
    section: 7,
    channel: 11,
    controller: 7,
    remove: [10, 11],
  });
});
it("exposes Tempo lane and commits tempoPoints for selected meta", async () => {
  const tempoView = {
    ...view,
    events: [
      {
        id: 40,
        tick: 1920,
        order: 40,
        data: [0xff, 0x51, 3, 0x07, 0xa1, 0x20],
      },
    ],
  };
  render(
    <ControllerLane
      notes={notes}
      section={section}
      view={tempoView}
      channel={11}
      width={800}
      scale={0.2}
      offset={0}
      grid={120}
      disabled={false}
      playheadTick={null}
    />,
  );
  fireEvent.change(screen.getByLabelText("Controller lane"), {
    target: { value: "-5" },
  });
  expect(screen.getByLabelText(/^Tempo lane[.]/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Controller gesture"), {
    target: { value: "relative" },
  });
  const c = screen.getByLabelText(/^Tempo lane[.]/);
  fireEvent.keyDown(c, { key: "a", ctrlKey: true });
  fireEvent.change(screen.getByLabelText("Lane selection value"), {
    target: { value: "140" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Set selected" }));
  await waitFor(() => expect(mock.invoke).toHaveBeenCalledTimes(1));
  expect(command()).toEqual({
    type: "tempoPoints",
    section: 7,
    points: [{ tick: 1920, bpm: 140 }],
  });
});
it("a revision change cancels the current gesture", async () => {
  const ui = mount();
  const c = screen.getByLabelText(/^Velocity lane[.]/);
  gesture(c, 60, 48, 60, 20);
  await act(async () =>
    ui.rerender(
      <ControllerLane
        notes={notes}
        section={section}
        view={{ ...view, revision: 1 }}
        channel={11}
        width={800}
        scale={0.2}
        offset={0}
        grid={120}
        disabled={false}
        playheadTick={null}
      />,
    ),
  );
  fireEvent.pointerUp(c);
  expect(mock.invoke).not.toHaveBeenCalled();
});

