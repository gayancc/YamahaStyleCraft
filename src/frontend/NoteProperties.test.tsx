import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { NoteProperties } from "./NoteProperties";
import {
  formatNoteTime,
  formatNoteDuration,
  parseNoteDuration,
  validateNoteProperties,
} from "./notePropertiesDraft";
import type { View, Note } from "./contracts";
const mock = vi.hoisted(() => ({ current: {} as any, edit: vi.fn() }));
vi.mock("./store", () => ({ useStudio: { getState: () => mock.current } }));
const section = { id: 7, label: "Main A", start: 4800, end: 9120 };
const note: Note = {
  id: 4,
  offId: 5,
  track: 0,
  channel: 11,
  pitch: 60,
  velocity: 90,
  start: 4800,
  duration: 120,
};
const view: View = {
  name: "Properties",
  revision: 2,
  dirty: false,
  canUndo: false,
  canRedo: false,
  ppq: 480,
  tempo: 120,
  numerator: 3,
  denominator: 4,
  format: "SFF2",
  sections: [section],
  notes: [note],
  events: [],
  routes: [],
  chunks: [],
  warnings: [],
};
const close = vi.fn();
const props = {
  note,
  view,
  section,
  channel: 11,
  position: { x: 100, y: 200 },
  disabled: false,
  onClose: close,
};
const velocity = (value: string) => {
  fireEvent.click(
    screen.getByRole("button", {
      name: "Enter Note properties velocity value",
    }),
  );
  const input = screen.getByLabelText("Note properties velocity value");
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
};
beforeEach(() => {
  close.mockReset();
  mock.edit.mockReset().mockResolvedValue(undefined);
  mock.current = {
    view,
    sectionId: 7,
    channel: 11,
    busy: false,
    error: null,
    edit: mock.edit,
  };
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("keeps fields as drafts and applies all properties through one scoped transaction", async () => {
  render(<NoteProperties {...props} />);
  fireEvent.change(screen.getByLabelText("Time (bar:beat:tick)"), {
    target: { value: "2:1:0" },
  });
  fireEvent.change(screen.getByLabelText("Note properties pitch"), {
    target: { value: "62" },
  });
  velocity("100");
  fireEvent.change(screen.getByLabelText("Note properties duration"), {
    target: { value: "0:0:240" },
  });
  expect(mock.edit).not.toHaveBeenCalled();
  expect(
    (
      screen.getByLabelText(
        "Note properties source channel",
      ) as HTMLInputElement
    ).readOnly,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(close).toHaveBeenCalledOnce());
  expect(mock.edit).toHaveBeenCalledExactlyOnceWith({
    type: "pianoNotes",
    section: 7,
    channel: 11,
    moveControllers: false,
    updates: [{ id: 4, start: 6240, duration: 240, pitch: 62, velocity: 100 }],
  });
});

it("Cancel, Escape and unchanged Apply do not edit or create history", () => {
  const ui = render(<NoteProperties {...props} />);
  velocity("70");
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(close).toHaveBeenCalledOnce();
  expect(mock.edit).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(close).toHaveBeenCalledTimes(2);
  ui.unmount();
  render(<NoteProperties {...props} />);
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(close).toHaveBeenCalledTimes(3);
  expect(mock.edit).not.toHaveBeenCalled();
});

it("rejects stale context and surfaces backend rejection without closing", async () => {
  render(<NoteProperties {...props} />);
  mock.current.view = { ...view, revision: 3 };
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  expect(screen.getByRole("alert").textContent).toMatch(/context changed/);
  expect(mock.edit).not.toHaveBeenCalled();
  mock.current.view = view;
  velocity("80");
  mock.edit.mockImplementation(async () => {
    mock.current.error = "FIFO identity conflict";
  });
  fireEvent.click(screen.getByRole("button", { name: "Apply" }));
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain("FIFO"),
  );
  expect(close).not.toHaveBeenCalled();
});

it("uses the retained section meter and validates syntax, numeric domains and section fit", () => {
  const draft = { time: "2:1:0", pitch: "60", velocity: "90", duration: "120" };
  expect(formatNoteTime(6240, view, section)).toBe("2:1:0");
  expect(validateNoteProperties(draft, 4, view, section).start).toBe(6240);
  for (const patch of [
    { time: "1:4:0" },
    { time: "1:1:480" },
    { time: "0:1:0" },
    { time: "1:1:-1" },
    { time: "1.5:1:0" },
    { time: "999999999999999999:1:0" },
    { velocity: "" },
    { velocity: "0" },
    { pitch: "128" },
    { duration: "0" },
    { duration: "1.5" },
    { time: "3:3:479", duration: "2" },
  ]) {
    expect(() =>
      validateNoteProperties({ ...draft, ...patch }, 4, view, section),
    ).toThrow();
  }
  const meterView = {
    ...view,
    numerator: 4,
    denominator: 4,
    events: [
      {
        id: 50,
        tick: section.start,
        order: 50,
        data: [255, 0x58, 4, 6, 3, 24, 8],
      },
    ],
  };
  expect(
    validateNoteProperties({ ...draft, time: "1:6:239" }, 4, meterView, section)
      .start,
  ).toBe(6239);
  expect(() =>
    validateNoteProperties(
      { ...draft, time: "1:6:240" },
      4,
      meterView,
      section,
    ),
  ).toThrow();
});

it("opens a compact modeless utility, restores drafts with Reset, and allows Tab to leave", () => {
  render(<NoteProperties {...props} />);
  const panel = screen.getByRole("dialog");
  expect(panel.getAttribute("aria-modal")).toBe("false");
  expect(panel.closest(".modal-backdrop")).toBeNull();
  velocity("67");
  fireEvent.change(screen.getByLabelText("Note properties duration"), {
    target: { value: "0:1:0" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Reset" }));
  expect(
    screen
      .getByRole("slider", { name: "Note properties velocity" })
      .getAttribute("aria-valuenow"),
  ).toBe("90");
  expect(
    (screen.getByLabelText("Note properties duration") as HTMLInputElement)
      .value,
  ).toBe("0:0:120");
  expect(fireEvent.keyDown(panel, { key: "Tab" })).toBe(true);
  expect(mock.edit).not.toHaveBeenCalled();
});
it("moves with title-bar keyboard controls and closes when its source view changes", () => {
  const ui = render(<NoteProperties {...props} />);
  const panel = screen.getByRole("dialog");
  const before = parseFloat(panel.style.left);
  fireEvent.keyDown(
    screen.getByRole("button", { name: "Move note properties" }),
    { key: "ArrowRight" },
  );
  expect(parseFloat(panel.style.left)).toBe(before + 10);
  ui.rerender(<NoteProperties {...props} view={{ ...view, revision: 3 }} />);
  expect(close).toHaveBeenCalledOnce();
  expect(mock.edit).not.toHaveBeenCalled();
});
it("musical durations roundtrip at the true meter and reject invalid/overflow values", () => {
  for (const n of [1, 120, 480, 1440, 1561])
    expect(
      parseNoteDuration(formatNoteDuration(n, view, section), view, section),
    ).toBe(n);
  expect(formatNoteDuration(1561, view, section)).toBe("1:0:121");
  for (const text of [
    "0:0:0",
    "0:3:0",
    "0:0:480",
    "-1:0:0",
    "0:1.5:0",
    "99999999999999999:0:0",
    "120",
  ])
    expect(() => parseNoteDuration(text, view, section)).toThrow();
});
