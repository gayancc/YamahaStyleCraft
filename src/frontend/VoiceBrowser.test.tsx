import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { VoiceBrowser, VoiceName } from "./VoiceBrowser";
import { useStudio } from "./store";
import type {
  CatalogSummary,
  CatalogVoice,
  View,
  VoiceSearch,
} from "./contracts";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => mocks);
const section = { id: 7, label: "Main A", start: 480, end: 960 };
const view: View = {
  name: "Voice fixture",
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
const voice: CatalogVoice = {
  name: "Test Grand",
  category: "Piano",
  subcategory: "-",
  voiceType: "Regular",
  msb: 104,
  lsb: 21,
  program: 0,
  page: 3,
};
const summary: CatalogSummary = {
  edition: "Genos v2.00",
  sourceName: "fixture.pdf",
  sourceSha256: "verified",
  count: 1711,
};
const found: VoiceSearch = { total: 1, voices: [voice] };
const initial = (command: string) =>
  Promise.resolve(
    command === "voice_catalog_status"
      ? summary
      : command === "search_voice_catalog"
        ? found
        : command === "resolve_catalog_voice"
          ? null
          : command === "import_voice_catalog"
            ? summary
            : { ...view, revision: 1, dirty: true, canUndo: true },
  );
const apply = () => screen.getByRole("button", { name: "Apply voice" });
const choose = async () =>
  fireEvent.click(await screen.findByRole("button", { name: /Test Grand/ }));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.invoke.mockReset().mockImplementation(initial);
  mocks.open.mockResolvedValue("C:/DataList.pdf");
  useStudio.setState({
    view,
    sectionId: 7,
    channel: 11,
    busy: false,
    error: null,
  });
});
afterEach(cleanup);

it("selection previews a 1-based program and Cancel makes no Style changes", async () => {
  const close = vi.fn();
  render(<VoiceBrowser section={section} channel={11} onClose={close} />);
  await choose();
  expect(
    screen.getByLabelText("Voice selection preview").textContent,
  ).toContain("Program 1");
  expect(screen.getByText(/following sections inherit/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(close).toHaveBeenCalledOnce();
  expect(useStudio.getState().view).toBe(view);
  expect(
    mocks.invoke.mock.calls.some(([c]) => c === "apply_catalog_voice"),
  ).toBe(false);
});
it("Apply sends one atomic voice address with zero-based program only after confirmation", async () => {
  const close = vi.fn();
  render(<VoiceBrowser section={section} channel={11} onClose={close} />);
  expect((apply() as HTMLButtonElement).disabled).toBe(true);
  await choose();
  fireEvent.click(apply());
  await waitFor(() => expect(close).toHaveBeenCalledOnce());
  expect(mocks.invoke).toHaveBeenCalledWith("apply_catalog_voice", {
    section: 7,
    channel: 11,
    revision: 0,
    address: { msb: 104, lsb: 21, program: 0 },
  });
  expect(useStudio.getState().view?.revision).toBe(1);
});
it("failed PDF import retains the existing searchable catalog and selected source", async () => {
  mocks.invoke.mockImplementation((cmd) =>
    cmd === "import_voice_catalog"
      ? Promise.reject("unsupported PDF edition")
      : initial(cmd),
  );
  render(<VoiceBrowser section={section} channel={11} onClose={vi.fn()} />);
  await choose();
  fireEvent.click(
    screen.getByRole("button", { name: "Import Data List PDF…" }),
  );
  expect((await screen.findByRole("alert")).textContent).toContain(
    "unsupported PDF",
  );
  expect(screen.getByRole("button", { name: /Test Grand/ })).toBeTruthy();
  expect(useStudio.getState().view).toBe(view);
  expect(useStudio.getState().busy).toBe(false);
});
it("cancelled PDF chooser leaves catalog and document untouched", async () => {
  mocks.open.mockResolvedValue(null);
  render(<VoiceBrowser section={section} channel={11} onClose={vi.fn()} />);
  await choose();
  fireEvent.click(
    screen.getByRole("button", { name: "Import Data List PDF…" }),
  );
  await waitFor(() => expect(useStudio.getState().busy).toBe(false));
  expect(
    mocks.invoke.mock.calls.some(([c]) => c === "import_voice_catalog"),
  ).toBe(false);
});
it("stale equal-revision replacement disables Apply", async () => {
  render(<VoiceBrowser section={section} channel={11} onClose={vi.fn()} />);
  await choose();
  act(() =>
    useStudio.setState({ view: { ...view, name: "Other", revision: 0 } }),
  );
  expect((apply() as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole("alert").textContent).toContain("document changed");
});
it("pending native apply cannot be duplicated or dismissed with Escape", async () => {
  const close = vi.fn();
  let resolve!: (v: View) => void;
  mocks.invoke.mockImplementation((cmd) =>
    cmd === "apply_catalog_voice"
      ? new Promise<View>((r) => {
          resolve = r;
        })
      : initial(cmd),
  );
  render(<VoiceBrowser section={section} channel={11} onClose={close} />);
  await choose();
  fireEvent.click(apply());
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  fireEvent.pointerDown(document.querySelector(".modal-backdrop")!);
  expect(close).not.toHaveBeenCalled();
  expect(
    mocks.invoke.mock.calls.filter(([c]) => c === "apply_catalog_voice"),
  ).toHaveLength(1);
  await act(async () => resolve({ ...view, revision: 1 }));
  expect(close).toHaveBeenCalledOnce();
});
it("older search responses cannot replace newer results or leave a hidden selection active", async () => {
  let oldResolve!: (r: VoiceSearch) => void;
  mocks.invoke.mockImplementation((cmd, args) =>
    cmd === "search_voice_catalog" && args.query === "old"
      ? new Promise<VoiceSearch>((r) => {
          oldResolve = r;
        })
      : cmd === "search_voice_catalog" && args.query === "empty"
        ? Promise.resolve({ total: 0, voices: [] })
        : initial(cmd),
  );
  render(<VoiceBrowser section={section} channel={11} onClose={vi.fn()} />);
  await choose();
  const input = screen.getByLabelText("Search voices");
  fireEvent.change(input, { target: { value: "old" } });
  await waitFor(() => expect(oldResolve).toBeTruthy());
  fireEvent.change(input, { target: { value: "empty" } });
  await screen.findByText(/No voices match/);
  await act(async () => oldResolve(found));
  expect(screen.queryByRole("button", { name: /Test Grand/ })).toBeNull();
  expect((apply() as HTMLButtonElement).disabled).toBe(true);
});
it("an old startup catalog read cannot overwrite a successfully imported catalog", async () => {
  let resolve!: (v: CatalogSummary | null) => void;
  mocks.invoke.mockImplementation((cmd) =>
    cmd === "voice_catalog_status"
      ? new Promise<CatalogSummary | null>((r) => {
          resolve = r;
        })
      : initial(cmd),
  );
  render(<VoiceBrowser section={section} channel={11} onClose={vi.fn()} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Import Data List PDF…" }),
  );
  await screen.findByLabelText("Search voices");
  await act(async () => resolve(null));
  expect(screen.getByLabelText("Search voices")).toBeTruthy();
  await choose();
});
it("the inspector resolves named voices again after undo or source changes", async () => {
  mocks.invoke.mockResolvedValue(voice);
  render(<VoiceName section={section} channel={11} onOpen={vi.fn()} />);
  await screen.findByText("Test Grand");
  act(() => useStudio.setState({ view: { ...view, revision: 2 } }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenLastCalledWith("resolve_catalog_voice", {
      section: 7,
      channel: 11,
      revision: 2,
    }),
  );
});
