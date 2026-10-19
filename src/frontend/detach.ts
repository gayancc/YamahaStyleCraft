import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emit, listen } from "@tauri-apps/api/event";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { View } from "./contracts";
import { isNoteClipboard, type NoteClipboard } from "./noteClipboard";

export const PIANO_ROLL_LABEL = "piano-roll";
export const MIXER_LABEL = "mixer";
export const VIEW_SYNC_EVENT = "studio://view-sync";
export const MIXER_SYNC_EVENT = "studio://mixer-sync";
export const CLIPBOARD_SYNC_EVENT = "studio://clipboard-sync";
export const MIDI_OWNERSHIP_EVENT = "studio://midi-ownership";

export type MidiOwnershipPayload = {
  owner: string;
  active: boolean;
  windows: number;
};

/** Safe under vitest mocks that omit `isTauri`. */
function tauriReady(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

export type ViewSyncPayload = {
  /** ProjectDoc slot — ignore sync from other project windows (SFM-008). */
  docLabel?: string;
  view: View | null;
  sectionId: number;
  channel: number;
  selected: number[];
};

export type MixerSyncPayload = {
  docLabel?: string;
  muted: number[];
  solo: number[];
  armedChannel: number | null;
  mixerApplyAll: boolean;
};

export type ClipboardSyncPayload = NoteClipboard;

export function docLabelFromUrl(): string {
  if (typeof window === "undefined") return "main";
  const doc = new URLSearchParams(window.location.search).get("doc");
  if (!doc || doc === "piano-roll" || doc === "mixer") return "main";
  return doc;
}

export function isDetachedPianoRoute(): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).get("detach") === "piano";
}

export function isDetachedMixerRoute(): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).get("detach") === "mixer";
}

async function waitCreated(win: WebviewWindow): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    win.once("tauri://created", () => resolve());
    win.once("tauri://error", (e) =>
      reject(new Error(String((e as { payload?: unknown }).payload ?? e))),
    );
  });
}

/** Open or focus the detached Piano Roll window (SFM-068). */
export async function openDetachedPianoRoll(docLabel = "main"): Promise<void> {
  if (!tauriReady()) {
    throw new Error("Detach Piano Roll requires the desktop app");
  }
  const label =
    docLabel === "main" ? PIANO_ROLL_LABEL : `${PIANO_ROLL_LABEL}-${docLabel}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.setFocus();
    return;
  }
  const qs =
    docLabel === "main"
      ? "/?detach=piano"
      : `/?detach=piano&doc=${encodeURIComponent(docLabel)}`;
  const win = new WebviewWindow(label, {
    url: qs,
    title: docLabel === "main" ? "Piano Roll" : `Piano Roll · ${docLabel}`,
    width: 1100,
    height: 720,
    minWidth: 640,
    minHeight: 420,
    backgroundColor: "#151719",
    focus: true,
  });
  await waitCreated(win);
}

/** Open or focus the detached Mixer window (SFM-084). */
export async function openDetachedMixer(docLabel = "main"): Promise<void> {
  if (!tauriReady()) {
    throw new Error("Detach Mixer requires the desktop app");
  }
  const label =
    docLabel === "main" ? MIXER_LABEL : `${MIXER_LABEL}-${docLabel}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.setFocus();
    return;
  }
  const qs =
    docLabel === "main"
      ? "/?detach=mixer"
      : `/?detach=mixer&doc=${encodeURIComponent(docLabel)}`;
  const win = new WebviewWindow(label, {
    url: qs,
    title: docLabel === "main" ? "Mixer" : `Mixer · ${docLabel}`,
    width: 980,
    height: 560,
    minWidth: 640,
    minHeight: 360,
    backgroundColor: "#151719",
    focus: true,
  });
  await waitCreated(win);
}

/**
 * Open an independent project editor window with its own Rust Session slot
 * (SFM-008 / SFM-MENU-FILE-NEW-WINDOW).
 */
export async function openProjectWindow(): Promise<string> {
  if (!tauriReady()) {
    throw new Error("New project window requires the desktop app");
  }
  for (let n = 2; n <= 8; n += 1) {
    const label = `project-${n}`;
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) continue;
    const win = new WebviewWindow(label, {
      url: `/?doc=${label}`,
      title: `Genos Style Studio · Project ${n}`,
      width: 1280,
      height: 800,
      minWidth: 900,
      minHeight: 600,
      backgroundColor: "#151719",
      focus: true,
    });
    await waitCreated(win);
    return label;
  }
  throw new Error("Maximum of 7 additional project windows is open");
}

export async function broadcastViewSync(
  payload: ViewSyncPayload,
): Promise<void> {
  if (!tauriReady()) return;
  try {
    await emit(VIEW_SYNC_EVENT, payload);
  } catch {
    /* best-effort; detached window can refresh via session_view */
  }
}

export async function listenViewSync(
  handler: (payload: ViewSyncPayload) => void,
): Promise<() => void> {
  if (!tauriReady()) return () => undefined;
  try {
    return await listen<ViewSyncPayload>(VIEW_SYNC_EVENT, (e) =>
      handler(e.payload),
    );
  } catch {
    return () => undefined;
  }
}

export async function broadcastMixerSync(
  payload: MixerSyncPayload,
): Promise<void> {
  if (!tauriReady()) return;
  try {
    await emit(MIXER_SYNC_EVENT, payload);
  } catch {
    /* best-effort */
  }
}

export async function listenMixerSync(
  handler: (payload: MixerSyncPayload) => void,
): Promise<() => void> {
  if (!tauriReady()) return () => undefined;
  try {
    return await listen<MixerSyncPayload>(MIXER_SYNC_EVENT, (e) =>
      handler(e.payload),
    );
  } catch {
    return () => undefined;
  }
}

export async function broadcastMidiOwnership(
  payload: MidiOwnershipPayload,
): Promise<void> {
  if (!tauriReady()) return;
  try {
    await emit(MIDI_OWNERSHIP_EVENT, payload);
  } catch {
    /* best-effort */
  }
}

export async function listenMidiOwnership(
  handler: (payload: MidiOwnershipPayload) => void,
): Promise<() => void> {
  if (!tauriReady()) return () => undefined;
  try {
    return await listen<MidiOwnershipPayload>(MIDI_OWNERSHIP_EVENT, (e) =>
      handler(e.payload),
    );
  } catch {
    return () => undefined;
  }
}

/** Cross-window note clipboard (SFM-009). */
export async function broadcastClipboardSync(
  payload: ClipboardSyncPayload,
): Promise<void> {
  if (!tauriReady()) return;
  await invoke("write_note_clipboard", { payload });
}

export async function listenClipboardSync(
  handler: (payload: ClipboardSyncPayload) => void,
): Promise<() => void> {
  if (!tauriReady()) return () => undefined;
  try {
    let received = false;
    const unlisten = await listen<ClipboardSyncPayload>(
      CLIPBOARD_SYNC_EVENT,
      (e) => {
        if (isNoteClipboard(e.payload)) {
          received = true;
          handler(e.payload);
        }
      },
    );
    try {
      const existing = await invoke<ClipboardSyncPayload | null>(
        "read_note_clipboard",
      );
      // A newer event may arrive while the initial snapshot is in flight.
      if (!received && isNoteClipboard(existing)) handler(existing);
    } catch {
      /* Existing live subscription remains usable. */
    }
    return unlisten;
  } catch {
    return () => undefined;
  }
}
