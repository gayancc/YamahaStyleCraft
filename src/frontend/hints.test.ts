import { describe, expect, it } from "vitest";
import { hintForElement, parseHint } from "./hints";

describe("parseHint", () => {
  it("splits a trailing keyboard shortcut from the control name", () => {
    expect(parseHint("Undo (Ctrl+Z)")).toEqual({ name: "Undo", shortcut: "Ctrl+Z" });
    expect(parseHint("Redo (Ctrl+Shift+Z)")).toEqual({ name: "Redo", shortcut: "Ctrl+Shift+Z" });
    expect(parseHint("Pan piano roll (3)")).toEqual({ name: "Pan piano roll", shortcut: "3" });
    expect(parseHint("Stop / release notes (Escape)")).toEqual({
      name: "Stop / release notes",
      shortcut: "Escape",
    });
    expect(parseHint("Move cursor to section end (End)")).toEqual({
      name: "Move cursor to section end",
      shortcut: "End",
    });
  });

  it("keeps descriptive parentheses in the name", () => {
    expect(parseHint("Detach Mixer (separate window)")).toEqual({
      name: "Detach Mixer (separate window)",
      shortcut: "",
    });
    expect(parseHint("Record / capture (arm R first)")).toEqual({
      name: "Record / capture (arm R first)",
      shortcut: "",
    });
    expect(parseHint("Event tick (section)")).toEqual({ name: "Event tick (section)", shortcut: "" });
  });

  it("handles empty and plain text", () => {
    expect(parseHint("")).toEqual({ name: "", shortcut: "" });
    expect(parseHint("  Play  ")).toEqual({ name: "Play", shortcut: "" });
  });
});

describe("hintForElement", () => {
  it("prefers title, then aria-label, and appends a live value readout", () => {
    const button = document.createElement("button");
    button.setAttribute("aria-label", "Tempo");
    expect(hintForElement(button)).toEqual({ name: "Tempo", shortcut: "", value: "" });
    button.title = "Tempo (T)";
    button.setAttribute("data-hint-value", "131 BPM");
    expect(hintForElement(button)).toEqual({ name: "Tempo", shortcut: "T", value: "131 BPM" });
  });

  it("returns an empty hint for no element", () => {
    expect(hintForElement(null)).toEqual({ name: "", shortcut: "", value: "" });
  });
});
