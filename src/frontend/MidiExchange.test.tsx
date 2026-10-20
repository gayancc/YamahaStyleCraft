import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { MidiExchange } from "./MidiExchange";
import { useStudio } from "./store";
import type { View, ExchangePreview } from "./contracts";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => mocks);
const section = { id: 7, label: "Main A", start: 480, end: 960 };
const view: View = {
  name: "Exchange",
  revision: 0,
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
};
const preview: ExchangePreview = {
  token: "preview-1",
  sourcePpq: 960,
  targetPpq: 480,
  sourceChannel: 1,
  targetChannel: 11,
  section: "Main A",
  originalNotes: 2,
  importedNotes: 3,
  ignoredEvents: 5,
  endTick: 470,
  unchanged: false,
  notePreview: [{ start: 10, pitch: 60, duration: 120, velocity: 91 }],
};
const choose = () =>
  fireEvent.click(screen.getByRole("button", { name: "Choose edited MIDI…" }));
const apply = () => screen.getByRole("button", { name: "Apply MIDI notes" });
beforeEach(() => {
  vi.clearAllMocks();
  useStudio.setState({
    view,
    sectionId: 7,
    channel: 11,
    busy: false,
    selected: [99],
  });
  mocks.open.mockResolvedValue("C:/phrase.mid");
  mocks.save.mockResolvedValue("C:/export.mid");
  mocks.invoke.mockResolvedValue(preview);
});
afterEach(cleanup);
it("previews replacement scope and Cancel never mutates the document", async () => {
  const close = vi.fn();
  render(<MidiExchange section={section} channel={11} onClose={close} />);
  expect((apply() as HTMLButtonElement).disabled).toBe(true);
  choose();
  await screen.findByLabelText("MIDI import preview");
  expect(mocks.invoke).toHaveBeenCalledWith("preview_phrase_midi", {
    path: "C:/phrase.mid",
    section: 7,
    channel: 11,
    revision: 0,
    wholeSection: false,
    docLabel: "main",
  });
  expect(screen.getByText("2 → 3")).toBeTruthy();
  expect(screen.getByText("960 → 480 PPQ")).toBeTruthy();
  expect(screen.getByText("Inspect 1 of 3 imported notes")).toBeTruthy();
  expect(screen.getByText("C4 (60)")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(close).toHaveBeenCalledOnce();
  expect(mocks.invoke).toHaveBeenCalledTimes(1);
  expect(useStudio.getState().view).toBe(view);
});
it("applies only the reviewed token and clears selection after native success", async () => {
  const close = vi.fn(),
    next = { ...view, revision: 1, dirty: true };
  mocks.invoke.mockImplementation((command) =>
    Promise.resolve(command === "apply_phrase_midi" ? next : preview),
  );
  render(<MidiExchange section={section} channel={11} onClose={close} />);
  choose();
  await screen.findByLabelText("MIDI import preview");
  fireEvent.click(apply());
  await waitFor(() => expect(close).toHaveBeenCalledOnce());
  expect(mocks.invoke).toHaveBeenLastCalledWith("apply_phrase_midi", {
    token: "preview-1",
  });
  expect(useStudio.getState().view).toEqual(next);
  expect(useStudio.getState().selected).toEqual([]);
  expect(useStudio.getState().busy).toBe(false);
});
it("reports rejected MIDI and prevents apply; a later valid file can be reviewed", async () => {
  mocks.invoke
    .mockRejectedValueOnce("MIDI contains unpaired note events")
    .mockResolvedValue(preview);
  render(<MidiExchange section={section} channel={11} onClose={vi.fn()} />);
  choose();
  expect((await screen.findByRole("alert")).textContent).toContain("unpaired");
  expect((apply() as HTMLButtonElement).disabled).toBe(true);
  choose();
  await screen.findByLabelText("MIDI import preview");
  expect(screen.queryByRole("alert")).toBeNull();
});
it("rejects a stale preview even when the replacement document has the same revision", async () => {
  render(<MidiExchange section={section} channel={11} onClose={vi.fn()} />);
  choose();
  await screen.findByLabelText("MIDI import preview");
  act(() =>
    useStudio.setState({
      view: { ...view, name: "Other document", revision: 0 },
    }),
  );
  expect((apply() as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole("alert").textContent).toContain("document changed");
  fireEvent.click(apply());
  expect(mocks.invoke).toHaveBeenCalledTimes(1);
});
it("does not import or export if the document changes while a file dialog is open", async () => {
  let resolve!: (path: string) => void;
  mocks.open.mockReturnValue(
    new Promise<string>((r) => {
      resolve = r;
    }),
  );
  render(<MidiExchange section={section} channel={11} onClose={vi.fn()} />);
  choose();
  await act(async () => {
    useStudio.setState({ view: { ...view, revision: 1 } });
    resolve("C:/phrase.mid");
  });
  expect(mocks.invoke).not.toHaveBeenCalled();
  expect(useStudio.getState().busy).toBe(false);
});
it("holds the dialog and blocks duplicate requests during native work", async () => {
  const close = vi.fn();
  let resolve!: (p: ExchangePreview) => void;
  mocks.invoke.mockReturnValue(
    new Promise<ExchangePreview>((r) => {
      resolve = r;
    }),
  );
  render(<MidiExchange section={section} channel={11} onClose={close} />);
  choose();
  await waitFor(() => expect(mocks.invoke).toHaveBeenCalledOnce());
  choose();
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  fireEvent.pointerDown(document.querySelector(".modal-backdrop")!);
  expect(close).not.toHaveBeenCalled();
  expect(mocks.invoke).toHaveBeenCalledOnce();
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
  expect(document.activeElement).toBe(screen.getByRole("dialog"));
  await act(async () => resolve(preview));
  expect(useStudio.getState().busy).toBe(false);
});
it("cancelled file selection performs no IPC and failed export keeps the dialog usable", async () => {
  mocks.open.mockResolvedValue(null);
  mocks.invoke.mockRejectedValue("destination already exists");
  render(<MidiExchange section={section} channel={11} onClose={vi.fn()} />);
  choose();
  await waitFor(() => expect(useStudio.getState().busy).toBe(false));
  expect(mocks.invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Export phrase MIDI…" }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "already exists",
  );
  expect(mocks.invoke).toHaveBeenCalledWith("export_phrase_midi", {
    path: "C:/export.mid",
    section: 7,
    channel: 11,
    revision: 0,
    wholeSection: false,
    docLabel: "main",
  });
  expect(useStudio.getState().view).toBe(view);
});
it("shows unchanged note comparison and traps focus without triggering background shortcuts", async () => {
  const close = vi.fn();
  mocks.invoke.mockResolvedValue({ ...preview, unchanged: true });
  render(<MidiExchange section={section} channel={11} onClose={close} />);
  choose();
  await screen.findByText(/Notes are unchanged/);
  apply().focus();
  fireEvent.keyDown(apply(), { key: "Tab" });
  expect(document.activeElement).toBe(
    screen.getByRole("checkbox", { name: "Entire section (all source channels)" }),
  );
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});
