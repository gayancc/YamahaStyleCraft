import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const win = vi.hoisted(() => ({
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
  startDragging: vi.fn(),
  isMaximized: vi.fn(async () => false),
  isDecorated: vi.fn(async () => false),
  onResized: vi.fn(async () => () => undefined),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));

import { WindowControls, titlebarDragProps } from "./WindowControls";

describe("WindowControls", () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(cleanup);

  it("draws minimize, maximize and close when the frame is removed", async () => {
    render(<WindowControls />);
    const group = await screen.findByRole("group", { name: "Window controls" });
    expect(group.querySelectorAll("button")).toHaveLength(3);
    await userEvent.click(screen.getByRole("button", { name: "Minimize" }));
    await userEvent.click(screen.getByRole("button", { name: "Maximize" }));
    await userEvent.click(screen.getByRole("button", { name: "Close window" }));
    expect(win.minimize).toHaveBeenCalled();
    expect(win.toggleMaximize).toHaveBeenCalled();
    expect(win.close).toHaveBeenCalled();
  });

  it("shows Restore while maximized and draws nothing with a native frame", async () => {
    win.isMaximized.mockResolvedValue(true);
    const { unmount } = render(<WindowControls />);
    expect(await screen.findByRole("button", { name: "Restore" })).toBeTruthy();
    unmount();
    win.isMaximized.mockResolvedValue(false);
    win.isDecorated.mockResolvedValue(true);
    render(<WindowControls />);
    await waitFor(() => expect(win.isDecorated).toHaveBeenCalled());
    expect(screen.queryByRole("group")).toBeNull();
    win.isDecorated.mockResolvedValue(false);
  });

  it("drags from empty title bar space but not from buttons", async () => {
    render(
      <header {...titlebarDragProps()}>
        <span>empty</span>
        <button>File</button>
      </header>,
    );
    await userEvent.pointer({ keys: "[MouseLeft>]", target: screen.getByText("empty") });
    expect(win.startDragging).toHaveBeenCalledTimes(1);
    await userEvent.pointer({ keys: "[MouseLeft>]", target: screen.getByText("File") });
    expect(win.startDragging).toHaveBeenCalledTimes(1);
  });
});
