import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { CatalogVoice, Command, Note, View } from "./contracts";
interface StudioState {
  view: View | null;
  sectionId: number;
  channel: number;
  selected: number[];
  busy: boolean;
  error: string | null;
  message: string;
  clipboard: Note[];
  snap: number;
  zoom: number;
  tool: "select" | "draw";
  /** Bank/program addresses recovered from expansion PACK MIDI setups. */
  expansionVoices: CatalogVoice[];
  setExpansionVoices: (voices: CatalogVoice[]) => void;
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
export const useStudio = create<StudioState>((set, get) => ({
  view: null,
  sectionId: 0,
  channel: 11,
  selected: [],
  busy: false,
  error: null,
  message: "Open a Yamaha Style to begin",
  clipboard: [],
  snap: 16,
  zoom: 1,
  tool: "select",
  expansionVoices: [],
  setExpansionVoices: (voices) => set({ expansionVoices: voices }),
  load: async (path) => {
    if (get().busy) return;
    set({ busy: true, error: null });
    try {
      const view = await invoke<View>("open_document", { path });
      set({
        view,
        sectionId:
          view.sections.find((s) => s.label === "Main A")?.id ??
          view.sections[0].id,
        selected: [],
        message: "Source loaded · original data preserved",
      });
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
      });
      set({
        view,
        sectionId:
          view.sections.find((s) =>
            s.label.toUpperCase().startsWith("MAIN "),
          )?.id ?? view.sections[0].id,
        selected: [],
        channel: 9,
        message:
          "Blank Style from reference · notes cleared · scaffold retained",
      });
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
      });
      set({
        view: next,
        selected: get().selected.filter((id) =>
          next.notes.some((n) => n.id === id),
        ),
        message: "Edit applied",
      });
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
        view: await invoke<View>("history", { redo }),
        selected: [],
        message: redo ? "Edit restored" : "Edit undone",
      });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ busy: false });
    }
  },
  select: (selected) => set({ selected }),
  section: (sectionId) => set({ sectionId, selected: [] }),
  setChannel: (channel) => set({ channel, selected: [] }),
  clearError: () => set({ error: null }),
}));
