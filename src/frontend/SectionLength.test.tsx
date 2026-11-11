import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { View } from "./contracts";
import { SectionLength } from "./SectionLength";

afterEach(cleanup);

function show(end: number) {
  const section = { id: 1, label: "Main A", start: 1920, end };
  const view: View = {
    name: "Meter change",
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
    events: [
      {
        id: 2,
        order: 2,
        tick: section.start,
        data: [255, 88, 4, 3, 2, 24, 8],
      },
    ],
    routes: [],
    chunks: [],
    warnings: [],
  };
  render(<SectionLength view={view} section={section} onClose={() => {}} />);
}

it("uses the meter in force at the section marker", () => {
  show(1920 + 3 * 1440);
  expect(screen.getByText("Main A · current 3 bars")).toBeTruthy();
  expect((screen.getByRole("spinbutton") as HTMLInputElement).value).toBe("2");
});

it("disables editing when the section does not span whole bars", () => {
  show(1920 + 2 * 1440 + 1);
  expect(screen.getByText(/needs a supported meter and a whole number of bars/)).toBeTruthy();
  expect((screen.getByRole("button", { name: "Shorten Section" }) as HTMLButtonElement).disabled).toBe(true);
});
