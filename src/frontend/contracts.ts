export interface Note {
  id: number;
  offId: number;
  track: number;
  channel: number;
  pitch: number;
  velocity: number;
  start: number;
  duration: number;
}
export interface Section {
  id: number;
  label: string;
  start: number;
  end: number;
}
export interface MidiEvent {
  id: number;
  tick: number;
  order: number;
  data: number[];
}
export interface Route {
  id: number;
  sections: string[];
  source: number;
  destination: number;
  bytes: number[];
  sourceRoot: number;
  sourceChord: number;
  middleNtr: number;
  middleNtt: number;
  middleNttBass: boolean;
}
export interface View {
  name: string;
  revision: number;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  ppq: number;
  format: string;
  tempo: number;
  numerator: number;
  denominator: number;
  sections: Section[];
  notes: Note[];
  events: MidiEvent[];
  routes: Route[];
  chunks: { tag: string; bytes: number }[];
  warnings: string[];
}
export interface NoteInput {
  track: number;
  channel: number;
  pitch: number;
  velocity: number;
  start: number;
  duration: number;
}
export type Command =
  | { type: "addNotes"; section: number; notes: NoteInput[] }
  | {
      type: "updateNotes";
      section: number;
      ids: number[];
      deltaTick?: number;
      deltaPitch?: number;
      velocity?: number;
      duration?: number;
      deltaDuration?: number;
    }
  | { type: "deleteNotes"; ids: number[] }
  | { type: "quantize"; section: number; ids: number[]; step: number }
  | {
      type: "casm";
      route: number;
      field: "root" | "chord" | "ntr" | "ntt";
      value: number;
    }
  | { type: "setEvent"; id: number; tick: number; data: number[] }
  | {
      type: "addEvent";
      section: number;
      track: number;
      tick: number;
      data: number[];
    }
  | { type: "deleteEvent"; id: number };
export interface Port {
  index: number;
  name: string;
}
export interface Devices {
  inputs: Port[];
  outputs: Port[];
}
export interface TransportStatus {
  playing: boolean;
  elapsedMs: number;
  error: string | null;
}
