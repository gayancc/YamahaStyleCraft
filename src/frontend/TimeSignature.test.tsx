import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TimeSignature } from "./TimeSignature";

describe("TimeSignature", () => {
  afterEach(cleanup);
  const setup = (n = 3, d = 4, disabled = false) => {
    const onApply = vi.fn();
    render(
      <TimeSignature
        numerator={n}
        denominator={d}
        disabled={disabled}
        onApply={onApply}
      />,
    );
    return onApply;
  };

  it("shows the signature and opens the key panel with the current values lit", () => {
    setup(6, 8);
    fireEvent.click(screen.getByRole("button", { name: "Time signature 6/8" }));
    expect(
      screen.getByRole("button", { name: "6 beats" }).getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      screen.getByRole("button", { name: "Beat unit 8" }).getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      screen.getByRole("button", { name: "Preset 6/8" }).getAttribute("aria-pressed"),
    ).toBe("true");
  });

  it("applies keys, presets, arrow keys and wheel halves", () => {
    const onApply = setup(3, 4);
    const chip = screen.getByRole("button", { name: "Time signature 3/4" });
    fireEvent.click(chip);
    fireEvent.click(screen.getByRole("button", { name: "Beat unit 8" }));
    expect(onApply).toHaveBeenLastCalledWith(3, 8);
    fireEvent.click(screen.getByRole("button", { name: "Preset 7/8" }));
    expect(onApply).toHaveBeenLastCalledWith(7, 8);
    fireEvent.keyDown(chip, { key: "ArrowUp" });
    expect(onApply).toHaveBeenLastCalledWith(4, 4);
    fireEvent.keyDown(chip, { key: "ArrowDown", shiftKey: true });
    expect(onApply).toHaveBeenLastCalledWith(3, 2);
  });

  it("clamps at the limits and closes on Escape", () => {
    const onApply = setup(16, 16);
    const chip = screen.getByRole("button", { name: "Time signature 16/16" });
    fireEvent.keyDown(chip, { key: "ArrowUp" });
    fireEvent.keyDown(chip, { key: "ArrowUp", shiftKey: true });
    expect(onApply).not.toHaveBeenCalled();
    fireEvent.click(chip);
    expect(screen.getByRole("dialog", { name: "Time signature" })).toBeTruthy();
    fireEvent.keyDown(chip, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
