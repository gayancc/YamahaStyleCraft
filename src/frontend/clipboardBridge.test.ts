import { beforeEach, expect, it, vi } from "vitest";
import { broadcastClipboardSync, listenClipboardSync } from "./detach";
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: () => true,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen,
  emit: vi.fn(),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({ WebviewWindow: vi.fn() }));
const old = {
  ppq: 480,
  notes: [
    {
      id: 1,
      offId: 2,
      track: 0,
      channel: 9,
      start: 0,
      duration: 120,
      pitch: 60,
      velocity: 90,
    },
  ],
};
const newer = { ...old, ppq: 1920 };
beforeEach(() => {
  vi.clearAllMocks();
  mocks.listen.mockResolvedValue(mocks.unlisten);
  mocks.invoke.mockResolvedValue(old);
});

it("hydrates a newly opened window from the application clipboard", async () => {
  const handler = vi.fn();
  const stop = await listenClipboardSync(handler);
  expect(mocks.invoke).toHaveBeenCalledWith("read_note_clipboard");
  expect(handler).toHaveBeenCalledWith(old);
  stop();
  expect(mocks.unlisten).toHaveBeenCalledOnce();
});

it("does not overwrite a live clipboard event with an older initialization response", async () => {
  let resolve!: (value: unknown) => void;
  mocks.invoke.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const handler = vi.fn();
  const pending = listenClipboardSync(handler);
  await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalled());
  mocks.listen.mock.calls[0][1]({ payload: newer });
  resolve(old);
  await pending;
  expect(handler).toHaveBeenCalledExactlyOnceWith(newer);
});

it("keeps the live subscription if the initial read fails and ignores malformed events", async () => {
  mocks.invoke.mockRejectedValue(Error("read unavailable"));
  const handler = vi.fn(),
    stop = await listenClipboardSync(handler);
  const receive = mocks.listen.mock.calls[0][1];
  receive({ payload: { notes: old.notes } });
  receive({ payload: old });
  expect(handler).toHaveBeenCalledExactlyOnceWith(old);
  stop();
  expect(mocks.unlisten).toHaveBeenCalledOnce();
});

it("writes centrally and propagates write failures so Cut can keep source notes", async () => {
  await broadcastClipboardSync(old);
  expect(mocks.invoke).toHaveBeenCalledWith("write_note_clipboard", {
    payload: old,
  });
  mocks.invoke.mockRejectedValue(Error("clipboard unavailable"));
  await expect(broadcastClipboardSync(old)).rejects.toThrow(
    "clipboard unavailable",
  );
});
