import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ParameterControl, formatPan } from "./ParameterControl";
beforeEach(() => {
  vi.stubGlobal("PointerEvent", MouseEvent);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => true);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
const setup = (extra = {}) => {
  const onCommit = vi.fn();
  const props = { label: "Pan", value: 64, resetValue: 64, onCommit, ...extra };
  const ui = render(<ParameterControl {...props} />);
  return {
    ...ui,
    onCommit,
    props,
    slider: screen.getByRole("slider", { name: "Pan" }),
  };
};
it("previews a bounded drag and commits only once on release", () => {
  const { slider, onCommit } = setup();
  fireEvent.pointerDown(slider, { button: 0, clientY: 200 });
  fireEvent.pointerMove(slider, { clientY: 150 });
  expect(Number(slider.getAttribute("aria-valuenow"))).toBeGreaterThan(64);
  expect(onCommit).not.toHaveBeenCalled();
  fireEvent.pointerMove(slider, { clientY: -900 });
  expect(slider.getAttribute("aria-valuenow")).toBe("127");
  fireEvent.pointerUp(slider);
  fireEvent.blur(slider);
  expect(onCommit).toHaveBeenCalledExactlyOnceWith(127);
});
it("Shift changes sensitivity without a jump when engaged", () => {
  const { slider } = setup();
  fireEvent.pointerDown(slider, { button: 0, clientY: 200 });
  fireEvent.pointerMove(slider, { clientY: 180 });
  const before = Number(slider.getAttribute("aria-valuenow"));
  fireEvent.pointerMove(slider, { clientY: 180, shiftKey: true });
  expect(Number(slider.getAttribute("aria-valuenow"))).toBe(before);
  fireEvent.pointerMove(slider, { clientY: 160, shiftKey: true });
  expect(Number(slider.getAttribute("aria-valuenow")) - before).toBeLessThan(3);
});
it("pointer cancellation and Escape restore the original without a history edit", () => {
  const { slider, onCommit } = setup();
  for (const cancel of [
    () => fireEvent.pointerCancel(slider),
    () => fireEvent.keyDown(slider, { key: "Escape" }),
  ]) {
    fireEvent.pointerDown(slider, { button: 0, clientY: 200 });
    fireEvent.pointerMove(slider, { clientY: 110 });
    cancel();
    fireEvent.pointerUp(slider);
    expect(slider.getAttribute("aria-valuenow")).toBe("64");
  }
  expect(onCommit).not.toHaveBeenCalled();
});
it("keyboard repeats form one gesture and boundaries/reset work", () => {
  const { slider, onCommit } = setup({ value: 40 });
  fireEvent.keyDown(slider, { key: "ArrowUp" });
  fireEvent.keyDown(slider, { key: "ArrowUp", repeat: true });
  expect(onCommit).not.toHaveBeenCalled();
  fireEvent.keyUp(slider, { key: "ArrowUp" });
  expect(onCommit).toHaveBeenLastCalledWith(42);
  fireEvent.keyDown(slider, { key: "Home" });
  fireEvent.keyUp(slider, { key: "Home" });
  expect(onCommit).toHaveBeenLastCalledWith(0);
  fireEvent.doubleClick(slider);
  expect(onCommit).toHaveBeenLastCalledWith(64);
});
it("exact entry rounds/clamps and Escape cancels without blur committing", () => {
  const { onCommit } = setup();
  fireEvent.click(screen.getByRole("button", { name: "Enter Pan value" }));
  let input = screen.getByLabelText("Pan value");
  fireEvent.change(input, { target: { value: "88" } });
  fireEvent.keyDown(input, { key: "Escape" });
  expect(onCommit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Enter Pan value" }));
  input = screen.getByLabelText("Pan value");
  fireEvent.change(input, { target: { value: "999" } });
  fireEvent.blur(input);
  expect(onCommit).toHaveBeenCalledExactlyOnceWith(127);
});
it("disabled controls ignore input; rejected native edit restores authoritative value", () => {
  const { slider, onCommit, rerender, props } = setup();
  fireEvent.keyDown(slider, { key: "ArrowUp" });
  fireEvent.keyUp(slider, { key: "ArrowUp" });
  expect(onCommit).toHaveBeenCalledWith(65);
  rerender(<ParameterControl {...props} disabled />);
  fireEvent.doubleClick(slider);
  fireEvent.keyDown(slider, { key: "End" });
  fireEvent.keyUp(slider, { key: "End" });
  expect(onCommit).toHaveBeenCalledTimes(1);
  rerender(<ParameterControl {...props} />);
  expect(slider.getAttribute("aria-valuenow")).toBe("64");
});
it("pan feedback expresses left/center/right and fader exposes real MIDI bounds", () => {
  setup({ variant: "fader", format: formatPan });
  const slider = screen.getByRole("slider");
  expect(slider.getAttribute("aria-valuetext")).toBe("C");
  expect(slider.getAttribute("aria-valuemin")).toBe("0");
  expect(slider.getAttribute("aria-valuemax")).toBe("127");
  expect(formatPan(0)).toBe("L 64");
  expect(formatPan(127)).toBe("R 63");
});

it("pending exact entry is discarded when editing becomes unavailable", () => {
  const { props, rerender, onCommit } = setup();
  fireEvent.click(screen.getByRole("button", { name: "Enter Pan value" }));
  fireEvent.change(screen.getByLabelText("Pan value"), {
    target: { value: "22" },
  });
  rerender(<ParameterControl {...props} disabled />);
  expect(screen.queryByLabelText("Pan value")).toBeNull();
  rerender(<ParameterControl {...props} />);
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.getByRole("slider").getAttribute("aria-valuenow")).toBe("64");
});
