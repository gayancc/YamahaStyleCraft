import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AdvancedQuantize } from "./AdvancedQuantize";
import type { View, Note } from "./contracts";
const mock = vi.hoisted(() => ({
  edit: vi.fn(),
  busy: false,
  setState: vi.fn(),
}));
vi.mock("./store", () => ({
  useStudio: { getState: () => mock, setState: mock.setState },
}));
const notes: Note[] = [
  {
    id: 1,
    offId: 2,
    track: 0,
    channel: 9,
    start: 50,
    duration: 100,
    pitch: 60,
    velocity: 90,
  },
];
const section = { id: 7, label: "Main A", start: 0, end: 480 };
const view: View = {
  name: "Quantize",
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
  notes,
  events: [],
  routes: [],
  chunks: [],
  warnings: [],
};
const props = {
  section,
  channel: 9,
  notes,
  selected: [] as number[],
  view,
  disabled: false,
};
const enterParameter = (label: string, value: string) => {
  fireEvent.click(screen.getByRole("button", { name: `Enter ${label} value` }));
  const input = screen.getByLabelText(`${label} value`);
  fireEvent.change(input, { target: { value } });
  fireEvent.blur(input);
};
beforeEach(() => {
  vi.useFakeTimers();
  mock.busy = false;
  mock.edit.mockReset().mockResolvedValue(undefined);
  mock.setState.mockReset();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("updates the editor grid for numeric sizes and preserves it for mixed grids", () => {
  render(<AdvancedQuantize {...props} />);
  fireEvent.change(screen.getByLabelText("Quantize size"), {
    target: { value: "320" },
  });
  expect(mock.setState).toHaveBeenLastCalledWith({ snap: 6 });
  mock.setState.mockClear();
  fireEvent.change(screen.getByLabelText("Quantize size"), {
    target: { value: "mixed8" },
  });
  expect(mock.setState).not.toHaveBeenCalled();
});

it("uses the atomic planner for a whole part or the exact selected identities", () => {
  const ui = render(<AdvancedQuantize {...props} />);
  fireEvent.click(screen.getByRole("button", { name: "Quantize whole part" }));
  expect(mock.edit).toHaveBeenLastCalledWith(
    expect.objectContaining({
      type: "advancedQuantize",
      section: 7,
      channel: 9,
      ids: [],
      options: expect.objectContaining({
        mode: "yamaha",
        target: "starts",
        fromOriginal: false,
      }),
    }),
  );
  ui.rerender(<AdvancedQuantize {...props} selected={[1]} />);
  fireEvent.click(screen.getByRole("button", { name: "Quantize selection" }));
  expect(mock.edit).toHaveBeenLastCalledWith(
    expect.objectContaining({ ids: [1] }),
  );
});

it("enforces conditional controls without erasing retained settings", () => {
  render(<AdvancedQuantize {...props} />);
  const strength = screen.getByLabelText(
    "Quantize strength",
  ) as HTMLInputElement;
  enterParameter("Quantize strength", "35");
  fireEvent.click(screen.getByLabelText("Quantize soft strength"));
  expect(strength.getAttribute("aria-disabled")).toBe("true");
  expect(strength.getAttribute("aria-valuenow")).toBe("35");
  expect(
    screen
      .getByLabelText("Quantize catch percent")
      .getAttribute("aria-disabled"),
  ).toBe("true");
  fireEvent.click(screen.getByLabelText("Quantize catch enabled"));
  expect(
    screen
      .getByLabelText("Quantize catch percent")
      .getAttribute("aria-disabled"),
  ).toBe("false");
  fireEvent.change(screen.getByLabelText("Quantize mode"), {
    target: { value: "relative" },
  });
  fireEvent.change(screen.getByLabelText("Quantize target"), {
    target: { value: "lengths" },
  });
  const mode = screen.getByLabelText("Quantize mode") as HTMLSelectElement;
  expect(mode.disabled).toBe(true);
  expect(mode.value).toBe("yamaha");
  expect(
    (
      screen.getByLabelText(
        "Move controllers with note starts",
      ) as HTMLInputElement
    ).disabled,
  ).toBe(true);
  fireEvent.change(screen.getByLabelText("Quantize size"), {
    target: { value: "mixed8" },
  });
  expect(
    (screen.getByLabelText("Quantize tuplet") as HTMLSelectElement).disabled,
  ).toBe(true);
  expect(
    screen.getByLabelText("Quantize swing").getAttribute("aria-disabled"),
  ).toBe("true");
});

it("debounces auto from originals and never loops on its own view revision", async () => {
  const ui = render(<AdvancedQuantize {...props} />);
  fireEvent.click(screen.getByLabelText("Quantize auto apply"));
  enterParameter("Quantize strength", "20");
  enterParameter("Quantize strength", "30");
  await act(() => vi.advanceTimersByTimeAsync(139));
  expect(mock.edit).not.toHaveBeenCalled();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(mock.edit).toHaveBeenCalledTimes(1);
  expect(mock.edit).toHaveBeenLastCalledWith(
    expect.objectContaining({
      options: expect.objectContaining({ strength: 30, fromOriginal: true }),
    }),
  );
  ui.rerender(<AdvancedQuantize {...props} view={{ ...view, revision: 1 }} />);
  await act(() => vi.advanceTimersByTimeAsync(500));
  expect(mock.edit).toHaveBeenCalledTimes(1);
});

it("cancels stale scope work and enables Reset only from saved original timing", async () => {
  const ui = render(<AdvancedQuantize {...props} />);
  expect(
    (
      screen.getByRole("button", {
        name: "Reset original",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  fireEvent.click(screen.getByLabelText("Quantize auto apply"));
  ui.rerender(
    <AdvancedQuantize
      {...props}
      section={{ ...section, id: 8 }}
      view={{
        ...view,
        quantizeOriginal: {
          "8:9": { notes: [{ id: 1, start: 50, duration: 100 }], events: [] },
        },
      }}
    />,
  );
  await act(() => vi.advanceTimersByTimeAsync(140));
  expect(mock.edit).toHaveBeenCalledTimes(1);
  expect(mock.edit).toHaveBeenLastCalledWith(
    expect.objectContaining({ section: 8 }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Reset original" }));
  expect(mock.edit).toHaveBeenLastCalledWith(
    expect.objectContaining({ section: 8, reset: true }),
  );
  ui.unmount();
  await act(() => vi.advanceTimersByTimeAsync(500));
  expect(mock.edit).toHaveBeenCalledTimes(2);
});
