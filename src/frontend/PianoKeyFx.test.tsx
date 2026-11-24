import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PianoKeyFx } from "./PianoKeyFx";

const props = { row: 18, width: 60, top: 28, height: 600, scrollY: 900 };

describe("PianoKeyFx", () => {
  it("lights only held and transport-sounding keys", () => {
    const { container } = render(
      <PianoKeyFx {...props} held={new Set([60])} playing={new Set([62])} />,
    );
    const key = (p: number) =>
      container.querySelector<HTMLElement>(`[data-pitch="${p}"]`)!;
    expect(key(60).className).toContain("on");
    expect(key(62).className).toContain("on");
    expect(key(64).className).not.toContain("on");
    expect(key(61).className).toContain("black");
  });

  it("keeps accidentals next to a lit white key dark", () => {
    const { container } = render(
      <PianoKeyFx {...props} held={new Set([62])} playing={new Set()} />,
    );
    const cls = (p: number) =>
      container.querySelector(`[data-pitch="${p}"]`)!.className;
    expect(cls(61)).toContain("mask");
    expect(cls(63)).toContain("mask");
    expect(cls(66)).not.toContain("mask");
  });

  it("restores the key at once on release (no fade classes)", () => {
    const { container, rerender } = render(
      <PianoKeyFx {...props} held={new Set([60])} playing={new Set()} />,
    );
    rerender(<PianoKeyFx {...props} held={new Set()} playing={new Set()} />);
    expect(container.querySelector('[data-pitch="60"]')!.className).toBe(
      "piano-key-fx-key",
    );
  });
});
