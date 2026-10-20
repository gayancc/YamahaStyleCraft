import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { focusableControls, restoreEditorFocus, trapModalFocus } from "./modal";
import { StudioMenu } from "./StudioMenu";

afterEach(cleanup);

it("cycles visible enabled controls and lets Escape reach the draft dismiss handler", () => {
  const dismiss = vi.fn();
  render(
    <div
      onKeyDown={(e) => {
        if (e.key === "Escape") dismiss();
      }}
    >
      <section role="dialog" onKeyDown={trapModalFocus} tabIndex={-1}>
        <button>First</button>
        <fieldset disabled>
          <input aria-label="Disabled by group" />
        </fieldset>
        <details>
          <summary>Advanced</summary>
          <input aria-label="Collapsed" />
        </details>
        <div hidden>
          <button>Hidden</button>
        </div>
        <textarea aria-label="Last" />
      </section>
    </div>,
  );
  const panel = screen.getByRole("dialog");
  expect(focusableControls(panel).map((el) => el.tagName)).toEqual([
    "BUTTON",
    "SUMMARY",
    "TEXTAREA",
  ]);
  const first = screen.getByText("First"),
    last = screen.getByLabelText("Last");
  last.focus();
  fireEvent.keyDown(last, { key: "Tab" });
  expect(document.activeElement).toBe(first);
  fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
  expect(document.activeElement).toBe(last);
  fireEvent.keyDown(last, { key: "Escape" });
  expect(dismiss).toHaveBeenCalledOnce();
});

it("navigates menu commands, skips disabled commands and prevents editor shortcuts", () => {
  const editorKey = vi.fn();
  render(
    <div onKeyDown={editorKey}>
      <StudioMenu label="File">
        <button>Open</button>
        <button disabled>Save</button>
        <button>New</button>
      </StudioMenu>
    </div>,
  );
  const summary = screen.getByText("File");
  summary.focus();
  fireEvent.keyDown(summary, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByText("Open"));
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  expect(document.activeElement).toBe(screen.getByText("New"));
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(summary.closest("details")!.open).toBe(false);
  expect(document.activeElement).toBe(summary);
  expect(editorKey).not.toHaveBeenCalled();
});

it("returns focus to the visible disclosure when a dialog command is hidden", () => {
  render(
    <details>
      <summary>File</summary>
      <button>New</button>
    </details>,
  );
  restoreEditorFocus(screen.getByText("New"));
  expect(document.activeElement).toBe(screen.getByText("File"));
});
