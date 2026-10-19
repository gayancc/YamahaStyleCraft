import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { SourceTracks } from "./SourceTracks";
import { useStudio } from "./store";
import type { Route, View } from "./contracts";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const bytes = Array(47).fill(0);
bytes[0] = 10;
bytes[9] = 10;
bytes[21] = 127;
bytes[29] = 138;
for (const o of [22, 28, 34]) bytes[o + 4] = 127;
bytes[17] = 128; // augmented chord is outside major/minor groups
const route: Route = {
  id: 0,
  source: 11,
  destination: 11,
  sections: ["Main A", "Main B"],
  bytes,
  sourceRoot: 0,
  sourceChord: 0,
  middleNtr: 0,
  middleNtt: 10,
  middleNttBass: true,
};
const section = { id: 7, label: "Main A", start: 10, end: 480 };
const view: View = {
  name: "Source dialog fixture",
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
  events: [{ id: 1, tick: 20, order: 1, data: [0xb2, 7, 100] }],
  routes: [route],
  chunks: [],
  warnings: [],
};
beforeEach(() => {
  vi.clearAllMocks();
  useStudio.setState({ view, sectionId: 7, busy: false, error: null });
  invoke.mockResolvedValue({
    ...view,
    revision: 1,
    dirty: true,
    canUndo: true,
  });
});
afterEach(cleanup);
const show = () => {
  const close = vi.fn();
  render(<SourceTracks route={route} section={section} onClose={close} />);
  return close;
};
describe("source-track draft transaction", () => {
  it("drafts bass defaults, accepts overrides back to original values, and applies once", async () => {
    show();
    fireEvent.click(screen.getByRole("button", { name: "Use part defaults" }));
    expect(invoke).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Source chord")).toHaveProperty("value", "2");
    expect(screen.getByLabelText("Output note low")).toHaveProperty(
      "value",
      "26",
    );
    expect(screen.getByLabelText("Output note high")).toHaveProperty(
      "value",
      "54",
    );
    expect(screen.getByLabelText("Region retrigger")).toHaveProperty(
      "value",
      "2",
    );
    // This equals the original route's chord. It still must override the reset.
    fireEvent.change(screen.getByLabelText("Source chord"), {
      target: { value: "0" },
    });
    fireEvent.click(screen.getByLabelText("Allow chord root C"));
    fireEvent.click(
      screen.getByRole("button", { name: "Apply source changes" }),
    );
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    expect(invoke).toHaveBeenCalledWith(
      "edit_document",
      expect.objectContaining({
        command: expect.objectContaining({
          actions: [
            expect.objectContaining({
              changes: [
                { type: "partDefaults" },
                { type: "rootEnabled", root: 0, enabled: false },
                { type: "chord", value: 0 },
              ],
              chords: null,
              exclusive: false,
            }),
          ],
        }),
      }),
    );
  });
  it("discards part defaults on Cancel without a native edit", () => {
    const close = show();
    fireEvent.click(screen.getByRole("button", { name: "Use part defaults" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(close).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
  it("edits root permissions and Auto Start in one section-local transaction", async () => {
    show();
    fireEvent.click(screen.getByLabelText("Allow chord root C"));
    fireEvent.click(screen.getByLabelText("Allow chord root B"));
    fireEvent.click(screen.getByLabelText("CASM Auto Start"));
    expect(invoke).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Apply source changes" }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "edit_document",
        expect.objectContaining({
          command: expect.objectContaining({
            type: "sourceTracks",
            section: 7,
            actions: [
              expect.objectContaining({
                changes: expect.arrayContaining([
                  { type: "rootEnabled", root: 0, enabled: true },
                  { type: "rootEnabled", root: 11, enabled: true },
                  { type: "autoStart", enabled: true },
                ]),
              }),
            ],
          }),
        }),
      ),
    );
  });
  it("Cancel and Escape discard edits without calling native IPC", () => {
    const close = show();
    fireEvent.change(screen.getByLabelText("Source root"), {
      target: { value: "7" },
    });
    fireEvent.click(screen.getByText("Cancel"));
    expect(close).toHaveBeenCalledOnce();
    expect(invoke).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(close).toHaveBeenCalledTimes(2);
  });
  it("submits final coherent zone and output-limit values after intermediate inversions", async () => {
    const close = show();
    fireEvent.change(screen.getByLabelText("Middle zone low"), {
      target: { value: "80" },
    });
    fireEvent.change(screen.getByLabelText("Middle zone high"), {
      target: { value: "60" },
    });
    expect(
      (screen.getByText("Apply source changes") as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.change(screen.getByLabelText("Middle zone low"), {
      target: { value: "36" },
    });
    fireEvent.change(screen.getByLabelText("Output note high"), {
      target: { value: "40" },
    });
    fireEvent.change(screen.getByLabelText("Output note low"), {
      target: { value: "50" },
    });
    fireEvent.change(screen.getByLabelText("Output note high"), {
      target: { value: "90" },
    });
    fireEvent.click(screen.getByText("Apply source changes"));
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    expect(invoke).toHaveBeenCalledWith("edit_document", {
      revision: 0,
      command: {
        type: "sourceTracks",
        section: 7,
        actions: [
          {
            type: "edit",
            source: 11,
            destination: 11,
            changes: [
              { type: "zone", low: 36, high: 60 },
              { type: "limits", region: "middle", low: 50, high: 90 },
            ],
            chords: null,
            exclusive: false,
          },
        ],
      },
      docLabel: "main",
    });
  });
  it("guitar transitions reset table index while keeping Bass and use the guitar table", async () => {
    show();
    fireEvent.change(screen.getByLabelText("Region NTR"), {
      target: { value: "2" },
    });
    expect(
      (screen.getByLabelText("Region NTT") as HTMLSelectElement).value,
    ).toBe("0");
    expect(
      (screen.getByLabelText("NTT Bass") as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.change(screen.getByLabelText("Region NTT"), {
      target: { value: "2" },
    });
    fireEvent.click(screen.getByText("Apply source changes"));
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(invoke.mock.calls[0][1].command.actions[0].changes).toEqual([
      { type: "rule", region: "middle", field: "ntr", value: 2 },
      { type: "rule", region: "middle", field: "ntt", value: 2 },
    ]);
  });
  it("major/minor groups preserve augmented; exclusive assignment and clone share one command", async () => {
    show();
    fireEvent.click(screen.getByText("Major group"));
    fireEvent.click(screen.getByText("Minor group"));
    expect(
      (screen.getByLabelText("Enable chord aug") as HTMLInputElement).checked,
    ).toBe(true);
    fireEvent.click(
      screen.getByLabelText("Assign enabled chords exclusively to this source"),
    );
    expect(
      (screen.getByRole("option", { name: "3 · in use" }) as HTMLOptionElement)
        .disabled,
    ).toBe(true);
    fireEvent.change(screen.getByLabelText("Clone into source channel"), {
      target: { value: "1" },
    });
    fireEvent.click(screen.getByText("Apply source changes"));
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    const actions = invoke.mock.calls[0][1].command.actions;
    expect(actions).toHaveLength(2);
    expect(actions[0].chords).toEqual(Array.from({ length: 17 }, (_, i) => i));
    expect(actions[0].exclusive).toBe(true);
    expect(actions[1]).toEqual({
      type: "clone",
      source: 11,
      destination: 11,
      target: 1,
    });
    expect(useStudio.getState().channel).toBe(1);
  });
  it("native rejection leaves the draft open with the entered values", async () => {
    invoke.mockRejectedValue("Unsupported sibling route");
    const close = show();
    fireEvent.change(screen.getByLabelText("Source root"), {
      target: { value: "7" },
    });
    fireEvent.click(screen.getByText("Apply source changes"));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "Unsupported sibling route",
      ),
    );
    expect(close).not.toHaveBeenCalled();
    expect(
      (screen.getByLabelText("Source root") as HTMLSelectElement).value,
    ).toBe("7");
  });
  it("rejects a stale draft and traps focus", () => {
    show();
    const cancel = screen.getByText("Cancel"),
      first = screen.getByLabelText("Close source tracks");
    first.focus();
    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(cancel);
    fireEvent.change(screen.getByLabelText("Source root"), {
      target: { value: "7" },
    });
    useStudio.setState({ view: { ...view, revision: 2 } });
    fireEvent.click(screen.getByText("Apply source changes"));
    expect(screen.getByRole("alert").textContent).toContain("document changed");
    expect(invoke).not.toHaveBeenCalled();
  });
});
