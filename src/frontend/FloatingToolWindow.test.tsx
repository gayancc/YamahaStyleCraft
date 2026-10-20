import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FloatingToolWindow } from "./FloatingToolWindow";
beforeEach(() =>
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  ),
);
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it("portals above clipped workspaces without an overlay or focus trap", () => {
  render(
    <div style={{ overflow: "hidden", height: 10 }}>
      <FloatingToolWindow
        open
        title="Quantize"
        context="Main A"
        onClose={() => {}}
      >
        <input aria-label="Grid value" />
      </FloatingToolWindow>
    </div>,
  );
  const panel = screen.getByRole("dialog", { name: "Quantize" });
  expect(panel.parentElement).toBe(document.body);
  expect(panel.getAttribute("aria-modal")).toBe("false");
  expect(fireEvent.keyDown(panel, { key: "Tab" })).toBe(true);
  expect(document.querySelector(".modal-backdrop")).toBeNull();
});
it("dismisses with Escape or close and preserves children while hidden", () => {
  const close = vi.fn();
  const props = { title: "Quantize", context: "Main A", onClose: close };
  const ui = render(
    <FloatingToolWindow {...props} open>
      <input aria-label="Grid value" defaultValue="120" />
    </FloatingToolWindow>,
  );
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: "Close Quantize" }));
  expect(close).toHaveBeenCalledTimes(2);
  const input = screen.getByLabelText("Grid value");
  fireEvent.change(input, { target: { value: "80" } });
  ui.rerender(
    <FloatingToolWindow {...props} open={false}>
      <input aria-label="Grid value" defaultValue="120" />
    </FloatingToolWindow>,
  );
  expect(screen.queryByRole("dialog")).toBeNull();
  ui.rerender(
    <FloatingToolWindow {...props} open>
      <input aria-label="Grid value" defaultValue="120" />
    </FloatingToolWindow>,
  );
  expect((screen.getByLabelText("Grid value") as HTMLInputElement).value).toBe(
    "80",
  );
});
it("returns focus to its opener after closing and raises the active utility", async () => {
  const opener = document.createElement("button");
  document.body.append(opener);
  opener.focus();
  const props = { title: "Tools", context: "Main A", onClose: () => {} };
  const ui = render(
    <FloatingToolWindow {...props} open>
      <input aria-label="Tool value" />
    </FloatingToolWindow>,
  );
  const panel = screen.getByRole("dialog");
  expect(document.activeElement).toBe(screen.getByLabelText("Tool value"));
  fireEvent.pointerDown(panel);
  expect(Number(panel.style.zIndex)).toBeGreaterThanOrEqual(60);
  ui.rerender(
    <FloatingToolWindow {...props} open={false}>
      <input aria-label="Tool value" />
    </FloatingToolWindow>,
  );
  await waitFor(() => expect(document.activeElement).toBe(opener));
  opener.remove();
});
