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
export interface ExchangePreview {
  token: string;
  sourcePpq: number;
  targetPpq: number;
  sourceChannel: number;
  targetChannel: number;
  section: string;
  originalNotes: number;
  importedNotes: number;
  ignoredEvents: number;
  endTick: number;
  unchanged: boolean;
  notePreview: {
    start: number;
    pitch: number;
    duration: number;
    velocity: number;
  }[];
}
export interface CatalogVoice {
  category: string;
  subcategory: string;
  name: string;
  msb: number;
  lsb: number;
  program: number;
  voiceType: string;
  page: number;
}
export interface CatalogSummary {
  edition: string;
  sourceName: string;
  sourceSha256: string;
  count: number;
}
export interface VoiceSearch {
  total: number;
  voices: CatalogVoice[];
}
export interface DrumChannelStatus {
  channel: number;
  address: { msb: number; lsb: number; program: number } | null;
  voice: CatalogVoice | null;
  lateProgram: boolean;
  kitCategory: boolean;
}
export interface EffectType {
  block: string;
  category: string;
  name: string;
  description: string;
  msb: number;
  lsb: number;
  parameterGroup: string;
  page: number;
}
export interface EffectSummary {
  edition: string;
  sourceName: string;
  sourceSha256: string;
  count: number;
  reverb: number;
  chorus: number;
  variation: number;
}
export interface EffectSearch {
  total: number;
  effects: EffectType[];
}
export type Region = "low" | "middle" | "high";
export type RuleField =
  "ntr" | "ntt" | "bass" | "highKey" | "noteLow" | "noteHigh" | "retrigger";
export type RouteChange =
  | { type: "partDefaults" }
  | { type: "rootEnabled"; root: number; enabled: boolean }
  | { type: "autoStart"; enabled: boolean }
  | { type: "limits"; region: Region; low: number; high: number }
  | { type: "root" | "chord"; value: number }
  | { type: "zone"; low: number; high: number }
  | { type: "rule"; region: Region; field: RuleField; value: number };
export type SourceAction =
  | {
      type: "edit";
      source: number;
      destination: number;
      changes: RouteChange[];
      chords: number[] | null;
      exclusive: boolean;
    }
  | { type: "clone"; source: number; destination: number; target: number };
export type Command =
  | {
      type: "setVoice";
      section: number;
      channel: number;
      address: { msb: number; lsb: number; program: number };
    }
  | {
      type: "replacePhrase";
      section: number;
      channel: number;
      notes: {
        start: number;
        pitch: number;
        duration: number;
        velocity: number;
      }[];
    }
  | { type: "sourceTracks"; section: number; actions: SourceAction[] }
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
  | { type: "deleteEvent"; id: number }
  | {
      type: "setTempoMeter";
      tempoBpm: number;
      numerator: number;
      denominator: number;
    };
export interface RecentEntry {
  id: string;
  path: string;
  name: string;
  kind: "sty" | "project";
  openedAt: string;
}
export interface RecentEntryView {
  entry: RecentEntry;
  available: boolean;
}
export interface Prefs {
  schema: string;
  midiOutputName: string | null;
  midiOutputIndex: number | null;
  snap: number;
  zoom: number;
  tool: string;
}
export interface MonitorEvent {
  stampMs: number;
  data: number[];
  summary: string;
}
export interface CaptureMarker {
  stampMs: number;
  label: string;
}
export interface CaptureSnapshot {
  recording: boolean;
  portName: string | null;
  events: MonitorEvent[];
  markers: CaptureMarker[];
  eventCount: number;
  droppedEvents: number;
}
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
