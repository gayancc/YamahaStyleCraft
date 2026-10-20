import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AddSection } from "./AddSection";
import { useStudio } from "./store";
import { layoutSections } from "./sectionSlots";
import type { View } from "./contracts";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const BAR = 1920; // 480 PPQ, 4/4
const sections = [
  { id: 7, label: "Main A", start: BAR, end: BAR * 5 },
  { id: 8, label: "Ending A", start: BAR * 5, end: BAR * 7 },
];
const view: View = {
  name: "Sparse Style",
  revision: 3,
  dirty: false,
  canUndo: false,
  canRedo: false,
  ppq: 480,
  format: "SFF2",
  tempo: 120,
  numerator: 4,
  denominator: 4,
  sections,
  notes: [],
  events: [],
  routes: [],
  chunks: [],
  warnings: [],
};
const slotOf = (key: string) => {
  const layout = layoutSections(sections);
  const group = layout.groups.find((g) => g.slots.some((s) => s.key === key))!;
  return { group: group.name, slot: group.slots.find((s) => s.key === key)! };
};

beforeEach(() => {
  vi.clearAllMocks();
  useStudio.setState({ view, sectionId: 7, busy: false, error: null });
  invoke.mockResolvedValue({ ...view, revision: 4, dirty: true, canUndo: true });
});
afterEach(cleanup);

const show = (key: string) => {
  const close = vi.fn();
  const { slot, group } = slotOf(key);
  render(<AddSection view={view} slot={slot} group={group} onClose={close} />);
  return close;
};
const lastCommand = () => invoke.mock.calls.at(-1)![1].command;

describe("AddSection dialog", () => {
  it("adds an empty section with the SFM default length and shares CASM with a section of the same family", async () => {
    const close = show("ending2");
    expect(screen.getByRole("dialog", { name: "Add Ending 2" })).toBeTruthy();
    expect((screen.getByLabelText("Length in bars") as HTMLInputElement).value).toBe("4");
    expect((screen.getByRole("radio", { name: "Empty section" }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText("Share CASM routes with") as HTMLSelectElement).value).toBe("8");
    fireEvent.click(screen.getByRole("button", { name: "Add section" }));
    await waitFor(() => expect(close).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith(
      "edit_document",
      expect.objectContaining({ revision: 3 }),
    );
    expect(lastCommand()).toEqual({
      type: "addSection",
      label: "Ending B",
      bars: 4,
      casmFrom: 8,
      copyFrom: null,
      fit: null,
    });
  });

  it("starts from a copy of an existing section: length follows the source and CASM follows the copy", async () => {
    show("mainB");
    fireEvent.click(screen.getByRole("radio", { name: "Copy of an existing section" }));
    fireEvent.change(screen.getByLabelText("Copy from section"), { target: { value: "8" } });
    expect((screen.getByLabelText("Length in bars") as HTMLInputElement).value).toBe("2");
    expect((screen.getByLabelText("Share CASM routes with") as HTMLSelectElement).value).toBe("8");
    fireEvent.change(screen.getByLabelText("Length in bars"), { target: { value: "4" } });
    expect((screen.getByLabelText("Fit copied timing") as HTMLSelectElement).value).toBe("repeat");
    fireEvent.click(screen.getByRole("button", { name: "Add section" }));
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(lastCommand()).toEqual({
      type: "addSection",
      label: "Main B",
      bars: 4,
      casmFrom: 8,
      copyFrom: 8,
      fit: "repeat",
    });
  });

  it("bounds fills and the break to 8 bars and blocks an invalid length", () => {
    show("fillA");
    const bars = screen.getByLabelText("Length in bars") as HTMLInputElement;
    expect(bars.value).toBe("1");
    expect(bars.max).toBe("8");
    fireEvent.change(bars, { target: { value: "9" } });
    expect((screen.getByRole("button", { name: "Add section" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("alert").textContent).toMatch(/1–8 bars/);
  });

  it("keeps the dialog open and shows the engine error when adding fails", async () => {
    invoke.mockRejectedValueOnce("the Style already has this section");
    const close = show("ending2");
    fireEvent.click(screen.getByRole("button", { name: "Add section" }));
    await screen.findByText(/already has this section/);
    expect(close).not.toHaveBeenCalled();
  });

  it("Escape closes without editing", () => {
    const close = show("ending2");
    fireEvent.keyDown(screen.getByRole("dialog", { name: "Add Ending 2" }), { key: "Escape" });
    expect(close).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
});
