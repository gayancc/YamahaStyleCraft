import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { askConfirm } from "./confirmDialog";
import { ConfirmHost } from "./ConfirmHost";

describe("ConfirmHost", () => {
  afterEach(cleanup);

  it("resolves true only when the confirm button is pressed", async () => {
    render(<ConfirmHost />);
    let result: Promise<boolean> = Promise.resolve(false);
    act(() => {
      result = askConfirm("Discard unsaved edits?", {
        title: "Unsaved Style",
        confirmLabel: "Discard",
      });
    });
    const dialog = await screen.findByRole("alertdialog", { name: "Unsaved Style" });
    expect(dialog.textContent).toContain("Discard unsaved edits?");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
    await userEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(await result).toBe(true);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("cancels on Escape and shows queued requests one at a time", async () => {
    render(<ConfirmHost />);
    let first: Promise<boolean> = Promise.resolve(true);
    let second: Promise<boolean> = Promise.resolve(false);
    act(() => {
      first = askConfirm("First?");
      second = askConfirm("Second?");
    });
    await screen.findByText("First?");
    expect(screen.queryByText("Second?")).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(await first).toBe(false);
    await screen.findByText("Second?");
    await userEvent.click(screen.getByRole("button", { name: "OK" }));
    expect(await second).toBe(true);
  });
});
