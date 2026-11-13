import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FirmwareConversion } from "./FirmwareConversion";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

describe("FirmwareConversion", () => {
  afterEach(() => cleanup());
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.open.mockReset();
  });

  it("reports the approximate converter when no firmware file is loaded", async () => {
    mocks.invoke.mockResolvedValue({ active: false, path: null });
    render(<FirmwareConversion />);
    expect(
      await screen.findByText(/approximate chord conversion/i),
    ).toBeTruthy();
  });

  it("loads the chosen Genos update file and shows exact conversion", async () => {
    mocks.invoke.mockImplementation(async (cmd: string) =>
      cmd === "configure_firmware"
        ? { active: true, path: "D:/Genos/genos_213.prg" }
        : { active: false, path: null },
    );
    mocks.open.mockResolvedValue("D:/Genos/genos_213.prg");
    render(<FirmwareConversion />);
    fireEvent.click(
      await screen.findByRole("button", { name: /choose genos update file/i }),
    );
    expect(await screen.findByText(/exact genos 2\.13 conversion/i)).toBeTruthy();
    expect(mocks.invoke).toHaveBeenCalledWith("configure_firmware", {
      path: "D:/Genos/genos_213.prg",
    });
  });

  it("shows why a file was rejected and keeps the previous state", async () => {
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "configure_firmware") {
        throw "Expected the verified Genos 2.13 update image";
      }
      return { active: false, path: null };
    });
    mocks.open.mockResolvedValue("D:/other.prg");
    render(<FirmwareConversion />);
    fireEvent.click(
      await screen.findByRole("button", { name: /choose genos update file/i }),
    );
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(
        /verified Genos 2\.13 update image/,
      ),
    );
    expect(screen.getByText(/approximate chord conversion/i)).toBeTruthy();
  });

  it("does nothing when the file dialog is cancelled", async () => {
    mocks.invoke.mockResolvedValue({ active: false, path: null });
    mocks.open.mockResolvedValue(null);
    render(<FirmwareConversion />);
    fireEvent.click(
      await screen.findByRole("button", { name: /choose genos update file/i }),
    );
    await waitFor(() => expect(mocks.open).toHaveBeenCalled());
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "configure_firmware",
      expect.anything(),
    );
  });
});
