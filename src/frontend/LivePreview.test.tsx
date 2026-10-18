import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { LivePreview } from "./LivePreview";
import type { View } from "./contracts";
const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mock.invoke }));
const view = {
  revision: 1,
  notes: [{ channel: 1, start: 0, duration: 400 }],
  routes: [{ source: 1, destination: 16, sections: ["Main A"] }],
} as View;
const section = { id: 2, label: "Main A", start: 0, end: 1920 };
const output = { index: 1, name: "Genos" };
const report = {
  durationMs: 8000,
  noteOns: 4,
  changedPitches: 2,
  setupPackets: 5,
  timedPackets: 8,
  routes: [],
  steps: [{ label: "C Major", startMs: 0, endMs: 2000, notes: 1 }],
  coverage: [
    {
      area: "CASM",
      status: "limited",
      count: 2,
      detail: "Approximate conversion",
    },
  ],
  warnings: ["Unsupported DSP is not sent"],
};
const props = {
  view,
  section,
  output,
  transport: { playing: false, elapsedMs: 0, error: null },
  onTransport: vi.fn(),
  onClose: vi.fn(),
};
beforeEach(() => {
  mock.invoke.mockReset().mockResolvedValue({ token: "live:1", report });
  props.onClose.mockReset();
  props.onTransport.mockReset();
});
afterEach(cleanup);
it("does not transmit on open or check; applies source channels and progression to preflight", async () => {
  render(<LivePreview {...props} />);
  expect(mock.invoke).not.toHaveBeenCalled();
  expect(
    (
      screen.getByRole("button", {
        name: "Play progression",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Check live preview" }));
  await screen.findByText("Preview ready");
  expect(mock.invoke.mock.calls[0]).toEqual([
    "preview_live_style",
    {
      revision: 1,
      config: {
        section: 2,
        sources: [1],
        chords: [
          { root: 0, quality: "Major", bars: 1 },
          { root: 9, quality: "Minor", bars: 1 },
          { root: 5, quality: "Major", bars: 1 },
          { root: 7, quality: "Major", bars: 1 },
        ],
        effects: false,
        drums: false,
      },
    },
  ]);
  expect(screen.getByText("Unsupported DSP is not sent")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Play progression" }));
  await waitFor(() =>
    expect(props.onTransport).toHaveBeenCalledWith({
      playing: true,
      elapsedMs: 0,
      error: null,
    }),
  );
  expect(mock.invoke).toHaveBeenCalledWith("play_live_style", {
    token: "live:1",
    index: 1,
    name: "Genos",
    looping: false,
  });
});
it("invalidates a ready plan when progression, document or output changes", async () => {
  const r = render(<LivePreview {...props} />);
  fireEvent.click(screen.getByText("Check live preview"));
  await screen.findByText("Preview ready");
  fireEvent.change(screen.getByLabelText("Chord 1 root"), {
    target: { value: "2" },
  });
  expect(screen.getByText("Check required")).toBeTruthy();
  fireEvent.click(screen.getByText("Check live preview"));
  await screen.findByText("Preview ready");
  r.rerender(<LivePreview {...props} output={{ name: "other", index: 3 }} />);
  expect(screen.getByText("Check required")).toBeTruthy();
  r.rerender(<LivePreview {...props} view={{ ...view, revision: 2 }} />);
  expect(screen.getByText("Check required")).toBeTruthy();
});
it("discards a late preflight after the document changes", async () => {
  let resolve!: (v: unknown) => void;
  mock.invoke.mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const r = render(<LivePreview {...props} />);
  fireEvent.click(screen.getByText("Check live preview"));
  r.rerender(<LivePreview {...props} view={{ ...view, revision: 2 }} />);
  await act(async () => resolve({ token: "stale", report }));
  expect(screen.queryByText("Preview ready")).toBeNull();
});
it("surfaces unsupported rules and stops before closing on Escape", async () => {
  mock.invoke.mockRejectedValueOnce("Guitar NTR unsupported");
  render(<LivePreview {...props} />);
  fireEvent.click(screen.getByText("Check live preview"));
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    "Guitar NTR unsupported",
  );
  mock.invoke.mockResolvedValue(undefined);
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  await waitFor(() => expect(props.onClose).toHaveBeenCalled());
  expect(mock.invoke).toHaveBeenLastCalledWith("stop");
});
it("requires an output and locks configuration during playback while keeping Stop usable", async () => {
  const r = render(<LivePreview {...props} output={null} />);
  fireEvent.click(screen.getByText("Check live preview"));
  await screen.findByText("Preview ready");
  expect(
    (screen.getByText("Play progression") as HTMLButtonElement).disabled,
  ).toBe(true);
  r.rerender(
    <LivePreview
      {...props}
      transport={{ playing: true, elapsedMs: 400, error: null }}
    />,
  );
  expect(
    (screen.getByLabelText("Chord 1 root") as HTMLSelectElement).closest(
      "fieldset",
    )?.disabled,
  ).toBe(true);
  fireEvent.click(screen.getByText("Stop"));
  await waitFor(() => expect(mock.invoke).toHaveBeenLastCalledWith("stop"));
});

it("includes optional slash bass only after selection and invalidates its reviewed plan", async () => {
  render(<LivePreview {...props} />);
  expect(
    (screen.getByLabelText("Chord 1 bass") as HTMLSelectElement).value,
  ).toBe("");
  fireEvent.change(screen.getByLabelText("Chord 1 bass"), {
    target: { value: "4" },
  });
  fireEvent.click(screen.getByText("Check live preview"));
  await screen.findByText("Preview ready");
  expect(mock.invoke.mock.calls[0][1].config.chords[0]).toEqual({
    root: 0,
    quality: "Major",
    bars: 1,
    bass: 4,
  });
  fireEvent.change(screen.getByLabelText("Chord 1 bass"), {
    target: { value: "" },
  });
  expect(screen.getByText("Check required")).toBeTruthy();
  expect(
    (screen.getByText("Play progression") as HTMLButtonElement).disabled,
  ).toBe(true);
});
it("discards a late preflight when slash bass changes while it is running", async () => {
  let resolve!: (v: unknown) => void;
  mock.invoke.mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const r = render(<LivePreview {...props} />);
  fireEvent.click(screen.getByText("Check live preview"));
  // Programmatic event simulates an external state change despite the disabled fieldset.
  fireEvent.change(screen.getByLabelText("Chord 1 bass"), {
    target: { value: "7" },
  });
  await act(async () => resolve({ token: "old", report }));
  expect(screen.queryByText("Preview ready")).toBeNull();
  r.unmount();
});
