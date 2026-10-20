import { beforeEach, expect, it, vi } from "vitest";
import type { View } from "./contracts";
import { useStudio } from "./store";
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./detach", () => ({ broadcastViewSync: vi.fn(), broadcastClipboardSync: vi.fn() }));
beforeEach(() => {
  vi.resetAllMocks();
  useStudio.setState({ view: null, busy: false, snap: 16, zoom: 1, channel: 11, tool: "select" });
});
const view = (uiState: object) => ({ sections: [{ id: 1, label: "Main A" }], uiState }) as View;
it("restores snap off from a saved project", async () => {
  mocks.invoke.mockResolvedValue(view({ snap: 0, zoom: 1, tool: "pan" }));
  await useStudio.getState().load("saved.gss");
  expect(useStudio.getState().snap).toBe(0);
  expect(useStudio.getState().tool).toBe("pan");
});
it("ignores out-of-range project display settings", async () => {
  mocks.invoke.mockResolvedValue(view({ snap: 3.5, zoom: 1e100, channel: 1.5 }));
  await useStudio.getState().load("malformed.gss");
  expect(useStudio.getState().snap).toBe(16);
  expect(useStudio.getState().zoom).toBe(1);
  expect(useStudio.getState().channel).toBe(11);
});
