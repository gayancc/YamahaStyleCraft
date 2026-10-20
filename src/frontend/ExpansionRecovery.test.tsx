import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ExpansionRecovery } from "./ExpansionRecovery";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-dialog", () => mocks);
beforeEach(() => {
  vi.resetAllMocks();
  mocks.invoke.mockResolvedValue({ capabilities: [], blockers: [], notes: [], project: {
    title: "Analyzed pack", fileSize: 32, resources: [{title: "wave", kind: "aiff", size: 4}],
  }});
});
afterEach(cleanup);

it("presents file picker failures without an unhandled rejection", async () => {
  mocks.open.mockRejectedValueOnce("File picker unavailable");
  render(<ExpansionRecovery onClose={() => {}} />);
  fireEvent.click(screen.getAllByRole("button", { name: "Select" })[0]);
  await screen.findByText("File picker unavailable");
  expect(mocks.invoke).not.toHaveBeenCalled();
});

it("invalidates extraction results when the selected package changes", async () => {
  mocks.open.mockResolvedValueOnce("first.ppf").mockResolvedValueOnce("second.ppf");
  render(<ExpansionRecovery onClose={() => {}} />);
  fireEvent.click(screen.getAllByRole("button", { name: "Select" })[2]);
  await screen.findByText("first.ppf");
  fireEvent.click(screen.getByRole("button", { name: /Analyze/ }));
  await screen.findByRole("button", { name: "Extract all resources" });
  fireEvent.click(screen.getAllByRole("button", { name: "Select" })[2]);
  await screen.findByText("second.ppf");
  expect(screen.queryByRole("button", { name: "Extract all resources" })).toBeNull();
});

it("shows analysis failures without retaining extraction actions", async () => {
  mocks.open.mockResolvedValue("broken.ppf");
  mocks.invoke.mockRejectedValueOnce("invalid chunk");
  render(<ExpansionRecovery onClose={() => {}} />);
  fireEvent.click(screen.getAllByRole("button", { name: "Select" })[2]);
  await screen.findByText("broken.ppf");
  fireEvent.click(screen.getByRole("button", { name: /Analyze/ }));
  await screen.findByText("invalid chunk");
  expect(screen.queryByRole("button", { name: "Extract all resources" })).toBeNull();
});
