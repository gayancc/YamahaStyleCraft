import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  CatalogVoice,
  Command,
  Note,
  ProjectUiState,
  View,
} from "./contracts";
import { broadcastClipboardSync, broadcastViewSync } from "./detach";
import { isNoteClipboard } from "./noteClipboard";

/** Piano-roll edit tools (SFM-PR-TOOL-*): Select/Draw/Paint/Erase + Cursor(2)/Pan(3). */
export type StudioTool =
  "select" | "draw" | "paint" | "erase" | "cursor" | "pan";

function applyUiState(
  view: View,
  fallbackSection: number,
  fallbackChannel: number,
): Partial<{
  sectionId: number;
  channel: number;
  snap: number;
  zoom: number;
  tool: StudioTool;
}> {
  const ui = view.uiState;
  if (!ui) {
    return {
      sectionId: fallbackSection,
      channel: fallbackChannel,
    };
  }
  const tools = new Set(["select", "draw", "paint", "erase", "cursor", "pan"]);
  const sectionOk =
    typeof ui.sectionId === "number" &&
    view.sections.some((s) => s.id === ui.sectionId);
  const channelOk =
    typeof ui.channel === "number" && ui.channel >= 1 && ui.channel <= 16;
  return {
    sectionId: sectionOk ? ui.sectionId! : fallbackSection,
    channel: channelOk ? ui.channel! : fallbackChannel,
    ...(typeof ui.snap === "number" && ui.snap > 0 ? { snap: ui.snap } : {}),
    ...(typeof ui.zoom === "number" && ui.zoom > 0 ? { zoom: ui.zoom } : {}),
    ...(ui.tool && tools.has(ui.tool) ? { tool: ui.tool } : {}),
  };
}

export function currentUiState(s: {
  sectionId: number;
  channel: number;
  snap: number;
  zoom: number;
  tool: StudioTool;
}): ProjectUiState {
  return {
    sectionId: s.sectionId,
    channel: s.channel,
    snap: s.snap,
    zoom: s.zoom,
    tool: s.tool,
  };
}

interface StudioState {
  view: View | null;
  sectionId: number;
  channel: number;
  selected: number[];
  busy: boolean;
  error: string | null;
  message: string;
  clipboard: Note[];
  clipboardPpq: number | null;
  snap: number;
  zoom: number;
  tool: StudioTool;
  /** Window/doc slot label (`main`, `project-2`, …) for SFM-008. */
  docLabel: string;
  /** Bank/program addresses recovered from expansion PACK MIDI setups. */
  expansionVoices: CatalogVoice[];
  setExpansionVoices: (voices: CatalogVoice[]) => void;
  setDocLabel: (label: string) => void;
  /** Copy/cut notes and broadcast to other Genos windows (SFM-009). */
  setClipboard: (notes: Note[]) => Promise<boolean>;
  load: (path: string) => Promise<void>;
  createBlank: (
    referencePath: string,
    options: {
      name: string;
      tempoBpm: number;
      numerator: number;
      denominator: number;
    },
  ) => Promise<void>;
  edit: (command: Command) => Promise<void>;
  history: (redo: boolean) => Promise<void>;
  select: (ids: number[]) => void;
  section: (id: number) => void;
  setChannel: (channel: number) => void;
  clearError: () => void;
}

function syncDetached(get: () => StudioState) {
  const s = get();
  void broadcastViewSync({
    docLabel: s.docLabel,
    view: s.view,
    sectionId: s.sectionId,
    channel: s.channel,
    selected: s.selected,
  });
}
export const useStudio = create<StudioState>((set, get) => ({
  view: null,
  sectionId: 0,
  channel: 11,
  selected: [],
  busy: false,
  error: null,
  message: "Open a Yamaha Style to begin",
  clipboard: [],
  clipboardPpq: null,
  snap: 16,
  zoom: 1,
  tool: "select",
  docLabel: "main",
  expansionVoices: [],
  setExpansionVoices: (voices) => set({ expansionVoices: voices }),
  setDocLabel: (docLabel) => set({ docLabel }),
  setClipboard: async (notes) => {
    if (!notes.length) {
      set({ message: "Select one or more notes before copying." });
      return false;
    }
    const payload = {
      notes: notes.map((n) => ({ ...n })),
      ppq: get().view?.ppq,
    };
    if (!isNoteClipboard(payload)) return false;
    const previous = {
      clipboard: get().clipboard,
      clipboardPpq: get().clipboardPpq,
    };
    set({ clipboard: payload.notes, clipboardPpq: payload.ppq });
    try {
      await broadcastClipboardSync(payload);
      return true;
    } catch (e) {
      set({
        ...(get().clipboard === payload.notes ? previous : {}),
        error: `Copy failed: ${String(e)}`,
      });
      return false;
    }
  },
  load: async (path) => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try {
      const view = await invoke<View>("open_document", {
        path,
        docLabel: get().docLabel,
      });
      const fallbackSection =
        view.sections.find((s) => s.label === "Main A")?.id ??
        view.sections[0].id;
      set({
        view,
        ...applyUiState(view, fallbackSection, get().channel),
        selected: [],
        message: "Source loaded · original data preserved",
      });
      syncDetached(get);
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },
  createBlank: async (referencePath, options) => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try {
      const view = await invoke<View>("create_blank_document", {
        referencePath,
        options,
        docLabel: get().docLabel,
      });
      set({
        view,
        sectionId:
          view.sections.find((s) => s.label.toUpperCase().startsWith("MAIN "))
            ?.id ?? view.sections[0].id,
        selected: [],
        channel: 9,
        message:
          "Blank Style from reference · notes cleared · scaffold retained",
      });
      syncDetached(get);
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },
  edit: async (command) => {
    const { view, busy } = get();
    if (!view || busy) return;
    set({ busy: true, error: null });
    try {
      const next = await invoke<View>("edit_document", {
        revision: view.revision,
        command,
        docLabel: get().docLabel,
      });
      const previousIds = new Set(view.notes.map((n) => n.id));
      const added = next.notes
        .filter((n) => !previousIds.has(n.id))
        .map((n) => n.id);
      const nextIds = new Set(next.notes.map((n) => n.id));
      set({
        view: next,
        selected: added.length
          ? added
          : get().selected.filter((id) => nextIds.has(id)),
        message: "Edit applied",
      });
      syncDetached(get);
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },
  history: async (redo) => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try {
      set({
        view: await invoke<View>("history", {
          redo,
          docLabel: get().docLabel,
        }),
        selected: [],
        message: redo ? "Edit restored" : "Edit undone",
      });
      syncDetached(get);
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },
  select: (selected) => {
    set({ selected });
    syncDetached(get);
  },
  section: (sectionId) => {
    set({ sectionId, selected: [] });
    syncDetached(get);
  },
  setChannel: (channel) => {
    set({ channel, selected: [] });
    syncDetached(get);
  },
  clearError: () => set({ error: null }),
}));
