import { afterEach, describe, expect, it, vi } from "vitest";
import {
  MIXER_LABEL,
  PIANO_ROLL_LABEL,
  docLabelFromUrl,
  isDetachedMixerRoute,
  isDetachedPianoRoute,
  openDetachedMixer,
  openDetachedPianoRoll,
  openProjectWindow,
} from "./detach";

const mocks = vi.hoisted(() => ({
  isTauri: vi.fn(() => true),
  getByLabel: vi.fn(),
  once: vi.fn(),
  setFocus: vi.fn(),
  WebviewWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => mocks.isTauri(),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => {
  class FakeWin {
    label: string;
    constructor(label: string) {
      this.label = label;
    }
    once(event: string, cb: () => void) {
      mocks.once(event, cb);
      if (event === "tauri://created") queueMicrotask(cb);
    }
    setFocus = mocks.setFocus;
  }
  mocks.WebviewWindow.mockImplementation(
    (label: string) => new FakeWin(label),
  );
  return {
    WebviewWindow: Object.assign(
      function (label: string, opts: unknown) {
        return mocks.WebviewWindow(label, opts);
      },
      { getByLabel: mocks.getByLabel },
    ),
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async () => () => undefined),
}));

afterEach(() => {
  vi.clearAllMocks();
  mocks.isTauri.mockReturnValue(true);
  window.history.replaceState({}, "", "/");
});

describe("detach routing helpers", () => {
  it("parses doc and detach query params", () => {
    window.history.replaceState({}, "", "/?detach=piano&doc=project-2");
    expect(isDetachedPianoRoute()).toBe(true);
    expect(isDetachedMixerRoute()).toBe(false);
    expect(docLabelFromUrl()).toBe("project-2");
    window.history.replaceState({}, "", "/?detach=mixer");
    expect(isDetachedMixerRoute()).toBe(true);
    expect(docLabelFromUrl()).toBe("main");
    window.history.replaceState({}, "", "/?doc=piano-roll");
    expect(docLabelFromUrl()).toBe("main");
  });

  it("opens per-doc Piano Roll / Mixer labels and focuses existing", async () => {
    mocks.getByLabel.mockResolvedValue(null);
    await openDetachedPianoRoll("project-3");
    expect(mocks.WebviewWindow).toHaveBeenCalledWith(
      `${PIANO_ROLL_LABEL}-project-3`,
      expect.objectContaining({
        url: "/?detach=piano&doc=project-3",
        title: "Piano Roll · project-3",
      }),
    );
    mocks.WebviewWindow.mockClear();
    const existing = { setFocus: mocks.setFocus };
    mocks.getByLabel.mockResolvedValue(existing);
    await openDetachedMixer("main");
    expect(mocks.setFocus).toHaveBeenCalled();
    expect(mocks.WebviewWindow).not.toHaveBeenCalled();
    expect(MIXER_LABEL).toBe("mixer");
  });

  it("opens the next free project-N window", async () => {
    mocks.getByLabel.mockImplementation(async (label: string) =>
      label === "project-2" ? { setFocus: vi.fn() } : null,
    );
    const label = await openProjectWindow();
    expect(label).toBe("project-3");
    expect(mocks.WebviewWindow).toHaveBeenCalledWith(
      "project-3",
      expect.objectContaining({ url: "/?doc=project-3" }),
    );
  });

  it("rejects detach when not running under Tauri", async () => {
    mocks.isTauri.mockReturnValue(false);
    await expect(openDetachedPianoRoll()).rejects.toThrow(/desktop app/);
    await expect(openProjectWindow()).rejects.toThrow(/desktop app/);
  });
});
