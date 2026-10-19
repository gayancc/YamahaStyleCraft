import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { NoteFxPanel } from "./NoteFxPanel";
import type { Note, View } from "./contracts";
afterEach(cleanup);
const section = { id: 1, label: "Main A", start: 0, end: 1920 };
const note: Note = {
  id: 1,
  offId: 2,
  track: 0,
  channel: 11,
  pitch: 60,
  velocity: 100,
  start: 120,
  duration: 480,
};
const view = { ppq: 480, events: [] } as unknown as View;
const setup = () => {
  const onApply = vi.fn();
  render(
    <NoteFxPanel
      view={view}
      section={section}
      channel={11}
      notes={[note]}
      disabled={false}
      onApply={onApply}
      onClear={vi.fn()}
    />,
  );
  return onApply;
};
it("the endpoint knobs feed the real note envelope planner", () => {
  const onApply = setup();
  fireEvent.click(
    screen.getByRole("button", { name: "Enter Note effect start value" }),
  );
  const input = screen.getByLabelText("Note effect start value");
  fireEvent.change(input, { target: { value: "14" } });
  fireEvent.blur(input);
  const end = screen.getByRole("slider", { name: "Note effect end" });
  fireEvent.keyDown(end, { key: "ArrowDown" });
  fireEvent.keyUp(end, { key: "ArrowDown" });
  fireEvent.click(
    screen.getByRole("button", { name: "Apply to selected notes" }),
  );
  const command = onApply.mock.calls[0][0];
  expect(command.controller).toBe(91);
  expect(command.points[0]).toEqual({ tick: 120, value: 14 });
  expect(command.points.some((p: { value: number }) => p.value === 71)).toBe(
    true,
  );
});
it("sustain replaces continuous endpoints with switches and emits only off/on values", () => {
  const onApply = setup();
  fireEvent.change(screen.getByLabelText("Note effect controller"), {
    target: { value: "64" },
  });
  expect(screen.queryByRole("slider")).toBeNull();
  fireEvent.click(
    screen.getByRole("switch", { name: "Note effect start sustain" }),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Apply to selected notes" }),
  );
  const command = onApply.mock.calls[0][0];
  expect(command.controller).toBe(64);
  expect(command.points[0].value).toBe(127);
  expect(
    command.points.every(
      (p: { value: number }) => p.value === 0 || p.value === 127,
    ),
  ).toBe(true);
});
