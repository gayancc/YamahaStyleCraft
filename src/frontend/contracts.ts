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
/** Workspace fields stored in `.gss` project extensions (SFM-002). */
export interface ProjectUiState {
  sectionId?: number;
  channel?: number;
  snap?: number;
  zoom?: number;
  tool?: "select" | "draw" | "paint" | "erase" | "cursor" | "pan";
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
  uiState?: ProjectUiState | null;
  quantizeOriginal?: Record<
    string,
    {
      notes: { id: number; start: number; duration: number }[];
      events: { id: number; tick: number; ownerNoteId: number }[];
    }
  > | null;
}
export type QuantizeGrid =
  | "480"
  | "320"
  | "240"
  | "160"
  | "120"
  | "80"
  | "60"
  | "40"
  | "mixed8"
  | "mixed16";
export interface QuantizeOptions {
  target: "starts" | "ends" | "lengths" | "both";
  mode: "yamaha" | "relative";
  size: QuantizeGrid;
  soft: boolean;
  strength: number;
  tuplet: number;
  swing: number;
  catchEnabled: boolean;
  catchPercent: number;
  safeRange: number;
  rough: number;
  moveControllers: boolean;
  fromOriginal: boolean;
  sourceFiltered: boolean;
}
export interface NoteInput {
  track: number;
  channel: number;
  pitch: number;
  velocity: number;
  start: number;
  duration: number;
}
export type NoteUpdate = Pick<
  Note,
  "id" | "start" | "duration" | "pitch" | "velocity"
>;
export interface ControllerPoint {
  tick: number;
  value: number;
}
/** 14-bit pitch bend (0..=16383, center 8192). */
export interface PitchBendPoint {
  tick: number;
  value: number;
}
export interface TempoPoint {
  tick: number;
  bpm: number;
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
  channels?: {
    channel: number;
    originalNotes: number;
    importedNotes: number;
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
  categories?: string[];
}
export interface VoiceSearch {
  total: number;
  voices: CatalogVoice[];
  offset?: number;
  hasMore?: boolean;
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
      type: "pianoNotes";
      section: number;
      channel: number;
      updates?: NoteUpdate[];
      add?: NoteInput[];
      remove?: number[];
      /** Default true: retime owned CC/pitch-bend when starts move. */
      moveControllers?: boolean;
    }
  | {
      type: "controllerPoints";
      section: number;
      channel: number;
      controller: number;
      points?: ControllerPoint[];
      remove?: number[];
    }
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
  | {
      type: "addNotes";
      section: number;
      notes: NoteInput[];
      channelEvents?: { track: number; tick: number; data: number[] }[];
    }
  | {
      type: "updateNotes";
      section: number;
      ids: number[];
      deltaTick?: number;
      deltaPitch?: number;
      velocity?: number;
      duration?: number;
      deltaDuration?: number;
      /** Default true: retime owned CC/pitch-bend with note starts. */
      moveControllers?: boolean;
    }
  | { type: "deleteNotes"; ids: number[] }
  | {
      type: "quantize";
      section: number;
      ids: number[];
      step: number;
      moveControllers?: boolean;
    }
  | {
      type: "advancedQuantize";
      section: number;
      channel: number;
      ids?: number[];
      options: Partial<QuantizeOptions>;
      reset?: boolean;
    }
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
    }
  | {
      type: "mixerCc";
      section: number;
      channel: number;
      controller: number;
      value: number;
      allSections?: boolean;
    }
  | {
      type: "mixerCcBatch";
      section: number;
      channel: number;
      changes: [number, number][];
      allSections?: boolean;
    }
  | {
      type: "mixerEq";
      section: number;
      channel: number;
      band: "eqLow" | "eqHigh";
      value: number;
      allSections?: boolean;
    }
  | {
      type: "mixerEqPoints";
      section: number;
      channel: number;
      band: "eqLow" | "eqHigh";
      points?: ControllerPoint[];
      clearTicks?: number[];
    }
  | {
      type: "pitchBendPoints";
      section: number;
      channel: number;
      points?: PitchBendPoint[];
      remove?: number[];
    }
  | {
      type: "tempoPoints";
      section: number;
      points?: TempoPoint[];
      remove?: number[];
    }
  | {
      type: "pitchBendRange";
      section: number;
      channel: number;
      semitones: number;
      allSections?: boolean;
    }
  | {
      type: "copyMaterial";
      sourceSection: number;
      destinationSection: number;
      scope:
        | { type: "section" }
        | { type: "part"; channel: number }
        | { type: "source"; channel: number };
      fit: "keep" | "repeat" | "fit";
    }
  | {
      type: "sectionLength";
      section: number;
      op:
        { type: "fillBreak"; bars: number } | { type: "shorten"; bars: number };
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
  /** Interface chrome scale 0.85–1.25 (SFM Settings A−/A+/100%). */
  uiScale?: number;
  tool: string;
  returnOnStop?: boolean;
  mixerApplyAllSections?: boolean;
  metronomeDuringPlayback?: boolean;
  countInBars?: number;
  recQuantize?: number;
  /** SFM Record data: `all` | `notes` | `controllers`. */
  recData?: "all" | "notes" | "controllers";
  /** 0 = OMNI, 1–16 = filter input channel before arm remap. */
  recInputChannel?: number;
  midiThru?: boolean;
  /** Positive pulls captured notes earlier (latency compensation). */
  recordingOffsetMs?: number;
  /** Favorite Style paths for the Part Library (SFM-LIB-FAVORITES). */
  libraryFavorites?: string[];
  /** Per MIDI channel 1–16 display colors (SFM Note colors). */
  noteColors?: string[];
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
