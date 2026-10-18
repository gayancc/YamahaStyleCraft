import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { open, save, confirm } from "@tauri-apps/plugin-dialog";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  FolderOpen,
  FilePlus2,
  Save,
  Download,
  Undo2,
  Redo2,
  Play,
  Square,
  SlidersHorizontal,
  Settings2,
  Music2,
  MousePointer2,
  Pencil,
  ZoomIn,
  ZoomOut,
  FileDown,
  Scissors,
  RefreshCw,
  X,
  ChevronRight,
  ChevronLeft,
  AudioLines,
  Repeat2,
  CircleHelp,
  CircleDot,
  OctagonAlert,
  Bell,
  FileJson,
  ListTree,
  PackageOpen,
} from "lucide-react";
import { useStudio } from "./store";
import type {
  CaptureSnapshot,
  Devices,
  MidiEvent,
  MonitorEvent,
  Port,
  Prefs,
  RecentEntryView,
  Route,
  TransportStatus,
  View,
} from "./contracts";
import { PianoRoll, pitchName } from "./PianoRoll";
import { SourceTracks } from "./SourceTracks";
import { MidiExchange } from "./MidiExchange";
import { VoiceBrowser, VoiceName } from "./VoiceBrowser";
import { DrumKits } from "./DrumKits";
import { EffectBrowser } from "./EffectBrowser";
import { InitialEffects, type EffectSelection } from "./InitialEffects";
import { LivePreview } from "./LivePreview";
import { ChordPreview } from "./ChordPreview";
import { ExpansionRecovery } from "./ExpansionRecovery";

const roles = [
  "Rhythm 1",
  "Rhythm 2",
  "Bass",
  "Chord 1",
  "Chord 2",
  "Pad",
  "Phrase 1",
  "Phrase 2",
];
const colors = [
  "#b694c7",
  "#9e86ba",
  "#dfb46c",
  "#76aaa5",
  "#76a3b7",
  "#979ec5",
  "#b1b880",
  "#c58f83",
];
function IconButton({
  label,
  children,
  onClick,
  disabled = false,
  active = false,
}: {
  label: string;
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  active?: boolean;
}) {
  return (
    <button
      title={label}
      aria-label={label}
      className={`icon-button ${active ? "active" : ""}`}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}
function NumberField({
  label,
  value,
  min = 0,
  max = 127,
  onCommit,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  onCommit: (n: number) => void;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <input
        aria-label={label}
        type="number"
        defaultValue={value}
        key={value}
        min={min}
        max={max}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") e.currentTarget.blur();
        }}
        onBlur={(e) => {
          const n = Number(e.target.value);
          if (
            e.target.value !== "" &&
            Number.isSafeInteger(n) &&
            n >= min &&
            n <= max
          ) {
            if (n !== value) onCommit(n);
          } else {
            e.target.value = String(value);
          }
        }}
      />
    </label>
  );
}
function Slider({
  label,
  value,
  onCommit,
}: {
  label: string;
  value: number;
  onCommit: (n: number) => void;
}) {
  const [v, set] = useState(value);
  useEffect(() => set(value), [value]);
  const commit = () => {
    if (v !== value) onCommit(v);
  };
  return (
    <label className="mixer-slider">
      <span>{label}</span>
      <input
        aria-label={label}
        type="range"
        min="0"
        max="127"
        value={v}
        onChange={(e) => set(+e.target.value)}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <output>{v}</output>
    </label>
  );
}

export function App() {
  const state = useStudio();
  const { view, sectionId, channel, selected, busy, error, snap, zoom, tool } =
    state;
  const [tab, setTab] = useState<"notes" | "mixer">("notes"),
    [inspector, setInspector] = useState<"note" | "casm" | "events">("note");
  const [deviceOpen, setDeviceOpen] = useState(false),
    [devices, setDevices] = useState<Devices>({ inputs: [], outputs: [] }),
    [output, setOutput] = useState<Port | null>(null);
  const [transport, setTransport] = useState<TransportStatus>({
      playing: false,
      elapsedMs: 0,
      error: null,
    }),
    [loop, setLoop] = useState(false);
  const [muted, setMuted] = useState<number[]>([]),
    [solo, setSolo] = useState<number[]>([]);
  const [recovery, setRecovery] = useState(false),
    [diagnostics, setDiagnostics] = useState(false);
  const [composerOpen, setComposerOpen] = useState(false);
  const [exchangeOpen, setExchangeOpen] = useState(false);
  const [voiceOpen, setVoiceOpen] = useState(false);
  const [drumsOpen, setDrumsOpen] = useState(false);
  const [effectsOpen, setEffectsOpen] = useState(false);
  const [initialEffects, setInitialEffects] = useState<EffectSelection | null>(
    null,
  );
  useEffect(() => {
    setInitialEffects((previous) =>
      previous && (previous.view !== view || previous.output !== output)
        ? null
        : previous,
    );
  }, [view, output]);
  const [livePreviewOpen, setLivePreviewOpen] = useState(false);
  const [chordPreviewOpen, setChordPreviewOpen] = useState(false);
  const [sourcesRoute, setSourcesRoute] = useState<Route | null>(null);
  const [composer, setComposer] = useState({
    name: "My Yamaha Style",
    tempoBpm: 120,
    numerator: 4,
    denominator: 4,
    referencePath: "",
  });
  const [recent, setRecent] = useState<RecentEntryView[]>([]);
  const [monitorName, setMonitorName] = useState<string>("");
  const [monitorEvents, setMonitorEvents] = useState<MonitorEvent[]>([]);
  const [monitoring, setMonitoring] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [captureSnap, setCaptureSnap] = useState<CaptureSnapshot | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const [expansionOpen, setExpansionOpen] = useState(false);
  const [limitsOpen, setLimitsOpen] = useState(false);
  const section = view?.sections.find((s) => s.id === sectionId);
  const sectionNotes =
    view?.notes.filter(
      (n) => section && n.start >= section.start && n.start < section.end,
    ) ?? [];
  const sources = [
    ...new Set([
      9,
      10,
      11,
      12,
      13,
      14,
      15,
      16,
      ...sectionNotes.map((n) => n.channel),
      ...(view?.routes
        .filter((r) => r.sections.includes(section?.label ?? ""))
        .map((r) => r.source) ?? []),
    ]),
  ].sort((a, b) => a - b);
  const role = (ch: number) => {
    const dest =
      view?.routes.find(
        (r) => r.source === ch && r.sections.includes(section?.label ?? ""),
      )?.destination ?? ch;
    return roles[dest - 9] ?? `Source ${ch}`;
  };
  const hasRetainedCapture = !!(
    capturing ||
    captureSnap?.eventCount ||
    captureSnap?.markers.length
  );
  const allowReplace = async () => {
    if (view?.dirty) {
      return await confirm("Discard unsaved edits and open another file?", {
        title: "Unsaved Style",
        kind: "warning",
      });
    }
    if (hasRetainedCapture) {
      return await confirm("Discard the retained MIDI capture take?", {
        title: "Retained capture",
        kind: "warning",
      });
    }
    return true;
  };
  const refreshRecent = async () => {
    if (!isTauri()) return;
    try {
      setRecent(await invoke<RecentEntryView[]>("list_recent"));
    } catch {
      /* soft-fail empty home */
    }
  };
  const openPath = async (path: string) => {
    if (!(await allowReplace())) return;
    await state.load(path);
    await refreshRecent();
  };
  const openFile = async () => {
    try {
      if (!(await allowReplace())) return;
      const path = await open({
        multiple: false,
        filters: [
          {
            name: "Styles, MIDI and studio projects",
            extensions: ["sty", "prs", "sst", "pst", "fps", "mid", "gss"],
          },
        ],
      });
      if (typeof path === "string") {
        await state.load(path);
        await refreshRecent();
      }
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const removeRecent = async (id: string) => {
    try {
      setRecent(await invoke<RecentEntryView[]>("remove_recent", { id }));
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const applyTempoMeter = async (
    tempoBpm: number,
    numerator: number,
    denominator: number,
  ) => {
    await state.edit({
      type: "setTempoMeter",
      tempoBpm,
      numerator,
      denominator,
    });
  };
  const exportCsv = async () => {
    if (!view) return;
    if (!section) {
      useStudio.setState({
        error: "Select a Style section before exporting CSV",
      });
      return;
    }
    try {
      const path = await save({
        defaultPath: `${view.name}-${section.label}.csv`,
        filters: [{ name: "CSV notes", extensions: ["csv"] }],
      });
      if (!path) return;
      await invoke("export_section_csv", { path, section: section.id });
      useStudio.setState({ message: "Section notes exported as CSV" });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const exportJson = async () => {
    if (!view) return;
    if (!section) {
      useStudio.setState({
        error: "Select a Style section before exporting JSON",
      });
      return;
    }
    try {
      const path = await save({
        defaultPath: `${view.name}-${section.label}.json`,
        filters: [{ name: "JSON snapshot", extensions: ["json"] }],
      });
      if (!path) return;
      await invoke("export_section_json", { path, section: section.id });
      useStudio.setState({
        message: "Section snapshot exported as JSON (not SFM profile)",
      });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const panic = async () => {
    if (!output) {
      setDeviceOpen(true);
      await scan();
      return;
    }
    try {
      await invoke("stop");
      setTransport((t) => ({ ...t, playing: false }));
      await invoke("panic_release", {
        index: output.index,
        name: output.name,
        channels: null,
      });
      useStudio.setState({ message: "MIDI panic / release sent" });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const audition = async () => {
    if (!output) {
      setDeviceOpen(true);
      await scan();
      return;
    }
    try {
      await invoke("test_note", {
        index: output.index,
        name: output.name,
        channel: 1,
        pitch: 60,
        velocity: 80,
        holdMs: 250,
      });
      useStudio.setState({ message: `Test note → ${output.name}` });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const applyCapture = async () => {
    if (!view || !section) return;
    try {
      useStudio.setState({ busy: true, error: null });
      const next = await invoke<View>("apply_capture_to_section", {
        section: section.id,
        revision: view.revision,
      });
      useStudio.setState({
        view: next,
        message: "Capture notes applied to section (not Style analyze-build)",
      });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    } finally {
      useStudio.setState({ busy: false });
    }
  };
  const openComposer = async () => {
    if (!(await allowReplace())) return;
    setComposer({
      name: "My Yamaha Style",
      tempoBpm: 120,
      numerator: 4,
      denominator: 4,
      referencePath: "",
    });
    setComposerOpen(true);
  };
  const pickReference = async () => {
    try {
      const path = await open({
        multiple: false,
        title: "Select Yamaha Style scaffold (SFF2)",
        filters: [
          {
            name: "Yamaha Style",
            extensions: ["sty", "prs", "sst", "pst", "fps"],
          },
        ],
      });
      if (typeof path === "string") {
        setComposer((c) => ({ ...c, referencePath: path }));
      }
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const createBlank = async () => {
    if (!composer.referencePath) {
      useStudio.setState({
        error:
          "Choose a reference Style scaffold before creating a blank project",
      });
      return;
    }
    if (
      !Number.isFinite(composer.tempoBpm) ||
      composer.tempoBpm < 20 ||
      composer.tempoBpm > 400
    ) {
      useStudio.setState({ error: "Tempo must be between 20 and 400 BPM" });
      return;
    }
    if (
      !Number.isSafeInteger(composer.numerator) ||
      composer.numerator < 1 ||
      composer.numerator > 16
    ) {
      useStudio.setState({
        error: "Meter numerator must be an integer from 1 to 16",
      });
      return;
    }
    if (![1, 2, 4, 8, 16].includes(composer.denominator)) {
      useStudio.setState({
        error: "Meter denominator must be 1, 2, 4, 8, or 16",
      });
      return;
    }
    await state.createBlank(composer.referencePath, {
      name: composer.name.trim() || "My Yamaha Style",
      tempoBpm: composer.tempoBpm,
      numerator: composer.numerator,
      denominator: composer.denominator,
    });
    if (!useStudio.getState().error) {
      setComposerOpen(false);
      await refreshRecent();
    }
  };
  const saveFile = async (projectFile: boolean) => {
    if (!view || busy) return;
    try {
      const path = await save({
        defaultPath: `${view.name}${projectFile ? ".gss" : "-edited.STY"}`,
        filters: [
          {
            name: projectFile ? "Studio project" : "Yamaha Style",
            extensions: [projectFile ? "gss" : "sty"],
          },
        ],
      });
      if (!path) return;
      useStudio.setState({ busy: true, error: null });
      const next = await invoke<View>("save_document", { path, projectFile });
      useStudio.setState({
        view: next,
        message: projectFile
          ? "Project saved with source data"
          : "Style exported and validated",
      });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    } finally {
      useStudio.setState({ busy: false });
    }
  };
  const scan = async () => {
    try {
      setDevices(await invoke<Devices>("scan_devices"));
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const persistPrefs = async (next: Partial<Prefs> & { tool?: string }) => {
    if (!isTauri()) return;
    try {
      const current = await invoke<Prefs>("load_prefs");
      // MIDI output is owned by React state so a slow load_prefs cannot resurrect
      // a port the user just cleared.
      const midiOutputName =
        "midiOutputName" in next
          ? (next.midiOutputName ?? null)
          : (output?.name ?? null);
      const midiOutputIndex =
        "midiOutputIndex" in next
          ? (next.midiOutputIndex ?? null)
          : (output?.index ?? null);
      await invoke("save_prefs", {
        prefs: {
          ...current,
          midiOutputName,
          midiOutputIndex,
          snap: next.snap ?? state.snap,
          zoom: next.zoom ?? state.zoom,
          tool: next.tool ?? state.tool,
        },
      });
    } catch {
      /* prefs are best-effort */
    }
  };
  const stop = async () => {
    try {
      await invoke("stop");
      setTransport((t) => ({ ...t, playing: false }));
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const play = async () => {
    if (transport.playing) {
      await stop();
      return;
    }
    if (!output) {
      setDeviceOpen(true);
      await scan();
      return;
    }
    if (!section) return;
    try {
      await invoke("play", {
        section: section.id,
        channels: sources.filter(
          (ch) => !muted.includes(ch) && (!solo.length || solo.includes(ch)),
        ),
        index: output.index,
        name: output.name,
        looping: loop,
        effectsToken:
          initialEffects?.view === view && initialEffects.output === output
            ? initialEffects.token
            : null,
      });
      setTransport({ playing: true, elapsedMs: 0, error: null });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  useEffect(() => {
    if (!isTauri()) return;
    let canceled = false;
    const poll = () => {
      if (document.visibilityState === "hidden") return;
      void invoke<TransportStatus>("transport_status")
        .then((s) => {
          if (!canceled) {
            setTransport(s);
            if (s.error) useStudio.setState({ error: s.error });
          }
        })
        .catch((e) => {
          if (!canceled) useStudio.setState({ error: String(e) });
        });
    };
    const timer = setInterval(poll, 200);
    document.addEventListener("visibilitychange", poll);
    return () => {
      canceled = true;
      clearInterval(timer);
      document.removeEventListener("visibilitychange", poll);
    };
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    void invoke<boolean>("recovery_status")
      .then(setRecovery)
      .catch((e) => useStudio.setState({ error: String(e) }));
    void refreshRecent();
    void invoke<Prefs>("load_prefs")
      .then((p) => {
        useStudio.setState({
          snap: p.snap,
          zoom: p.zoom,
          tool: p.tool === "draw" ? "draw" : "select",
        });
        if (p.midiOutputName != null && p.midiOutputIndex != null) {
          setOutput({ name: p.midiOutputName, index: p.midiOutputIndex });
        }
      })
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    if (!monitoring || !isTauri()) return;
    const timer = setInterval(() => {
      void invoke<[string | null, MonitorEvent[]]>("monitor_snapshot")
        .then(([name, events]) => {
          setMonitorName(name ?? "");
          setMonitorEvents(events.slice(-40).reverse());
        })
        .catch(() => undefined);
    }, 400);
    return () => clearInterval(timer);
  }, [monitoring]);
  useEffect(() => {
    if (!isTauri()) return;
    void invoke<CaptureSnapshot>("capture_snapshot")
      .then((snap) => {
        setCaptureSnap(snap);
        setCapturing(snap.recording);
        if (snap.portName) setMonitorName(snap.portName);
      })
      .catch(() => undefined);
  }, [deviceOpen]);
  useEffect(() => {
    if (!capturing || !isTauri()) return;
    const timer = setInterval(() => {
      void invoke<CaptureSnapshot>("capture_snapshot")
        .then((snap) => {
          setCaptureSnap(snap);
          setCapturing(snap.recording);
          if (snap.portName) setMonitorName(snap.portName);
        })
        .catch(() => undefined);
    }, 400);
    return () => clearInterval(timer);
  }, [capturing]);
  const captureGuard = useRef({ capturing: false, retained: false });
  captureGuard.current = {
    capturing,
    retained: !!(captureSnap?.eventCount || captureSnap?.markers.length),
  };
  useEffect(() => {
    if (!isTauri()) return;
    const listener = getCurrentWindow().onCloseRequested(async (e) => {
      const dirty = useStudio.getState().view?.dirty;
      const retained =
        captureGuard.current.capturing || captureGuard.current.retained;
      if (dirty || retained) {
        e.preventDefault();
        if (
          await confirm(
            dirty
              ? "Close and discard unsaved changes?"
              : "Close and discard the retained MIDI capture take?",
            {
              title: dirty ? "Unsaved Style" : "Retained capture",
              kind: "warning",
            },
          )
        ) {
          await invoke("stop");
          await invoke("stop_capture").catch(() => undefined);
          await getCurrentWindow().destroy();
        }
      }
    });
    return () => {
      void listener.then((f) => f());
    };
  }, []);
  const keyHandlerRef = useRef<(e: KeyboardEvent) => void>(() => {});
  keyHandlerRef.current = (e: KeyboardEvent) => {
    if (
      deviceOpen ||
      diagnostics ||
      recovery ||
      composerOpen ||
      helpOpen ||
      limitsOpen ||
      expansionOpen ||
      exchangeOpen ||
      voiceOpen ||
      drumsOpen ||
      effectsOpen ||
      livePreviewOpen ||
      chordPreviewOpen ||
      sourcesRoute
    ) {
      if (e.key === "Escape" && !recovery) {
        if (
          (composerOpen ||
            exchangeOpen ||
            voiceOpen ||
            drumsOpen ||
            effectsOpen ||
            chordPreviewOpen) &&
          busy
        )
          return;
        e.preventDefault();
        setDeviceOpen(false);
        setDiagnostics(false);
        setComposerOpen(false);
        setHelpOpen(false);
        setLimitsOpen(false);
        setExpansionOpen(false);
        setExchangeOpen(false);
        setVoiceOpen(false);
        setDrumsOpen(false);
        setEffectsOpen(false);
        setChordPreviewOpen(false);
        setLivePreviewOpen(false);
        if (livePreviewOpen) void stop();
        setSourcesRoute(null);
      }
      return;
    }
    const input = (e.target as HTMLElement).closest("input,textarea,select");
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.key.toLowerCase() === "o") {
      e.preventDefault();
      void openFile();
    }
    if (ctrl && e.key.toLowerCase() === "n") {
      e.preventDefault();
      void openComposer();
    }
    if (ctrl && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void saveFile(!e.shiftKey);
    }
    if (!input && ctrl && e.key.toLowerCase() === "z") {
      e.preventDefault();
      void state.history(e.shiftKey);
    }
    if (!input && e.code === "Space") {
      e.preventDefault();
      void play();
    }
    if (e.key === "Escape") {
      setDeviceOpen(false);
      void stop();
    }
  };
  useEffect(() => {
    const handler = (e: KeyboardEvent) => keyHandlerRef.current(e);
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
  useEffect(() => {
    if (
      !deviceOpen &&
      !diagnostics &&
      !recovery &&
      !composerOpen &&
      !expansionOpen &&
      !limitsOpen &&
      !helpOpen
    )
      return;
    const previous = document.activeElement as HTMLElement | null;
    const modal = document.querySelector<HTMLElement>('[aria-modal="true"]');
    const controls = () =>
      Array.from(
        modal?.querySelectorAll<HTMLElement>(
          'button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex="0"]',
        ) ?? [],
      );
    controls()[0]?.focus();
    const trap = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const els = controls(),
        first = els[0],
        last = els.at(-1);
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first?.focus();
      }
    };
    window.addEventListener("keydown", trap, true);
    return () => {
      window.removeEventListener("keydown", trap, true);
      previous?.focus();
    };
  }, [deviceOpen, diagnostics, recovery, composerOpen]);
  const cc = (ch: number, controller: number, value: number) =>
    setDiscrete(ch, [0xb0 | (ch - 1), controller, value]);
  const setDiscrete = (ch: number, data: number[]) => {
    if (!view || !section) return;
    const existing = [...view.events]
      .reverse()
      .find(
        (e) =>
          e.tick === section.start &&
          e.data[0] === data[0] &&
          (data[0] >> 4 !== 0xb || e.data[1] === data[1]),
      );
    if (existing)
      void state.edit({
        type: "setEvent",
        id: existing.id,
        tick: existing.tick,
        data,
      });
    else
      void state.edit({
        type: "addEvent",
        section: section.id,
        track: sectionNotes.find((n) => n.channel === ch)?.track ?? 0,
        tick: section.start,
        data,
      });
  };
  const valueAt = (ch: number, kind: number, param?: number, fallback = 0) => {
    const events =
      view?.events
        .filter(
          (e) =>
            e.tick <= (section?.start ?? 0) &&
            e.data[0] === (kind | (ch - 1)) &&
            (param === undefined || e.data[1] === param),
        )
        .sort((a, b) => a.tick - b.tick || a.order - b.order) ?? [];
    const e = events.at(-1);
    return e ? e.data[param === undefined ? 1 : 2] : fallback;
  };
  const note = view?.notes.find((n) => selected.includes(n.id));
  const selectedNotes =
    view?.notes.filter((n) => selected.includes(n.id)) ?? [];
  const channelNotes = sectionNotes.filter((n) => n.channel === channel);
  const noteIndex = channelNotes.findIndex((n) => n.id === note?.id);
  const editingDisabled = busy || transport.playing;
  return (
    <div className="app">
      <header className="titlebar">
        <div className="brand">
          <AudioLines size={23} />
          <span>
            Genos<span className="brand-light">StyleStudio</span>
          </span>
          <small>0.1</small>
        </div>
        <div className="project-title">
          {view ? (
            <>
              {view.name}
              <span className={`save-state ${view.dirty ? "dirty" : ""}`}>
                {view.dirty ? "● Unsaved" : "Saved"}
              </span>
            </>
          ) : (
            <span>Yamaha Style editor</span>
          )}
        </div>
        <IconButton
          label="Expansion Pack Recovery"
          onClick={() => setExpansionOpen(true)}
        >
          <PackageOpen size={17} />
        </IconButton>
        <IconButton
          label="Feature inventory / limits"
          onClick={() => setLimitsOpen(true)}
        >
          <ListTree size={17} />
        </IconButton>
        <IconButton label="Help" onClick={() => setHelpOpen(true)}>
          <CircleHelp size={17} />
        </IconButton>
        <IconButton
          label="MIDI device settings"
          onClick={() => {
            setDeviceOpen(true);
            void scan();
          }}
        >
          <Settings2 size={17} />
        </IconButton>
      </header>
      <div className="toolbar">
        <div className="file-actions">
          <button onClick={() => void openFile()} disabled={busy}>
            <FolderOpen size={16} /> Open
          </button>
          <button onClick={() => void openComposer()} disabled={busy}>
            <FilePlus2 size={16} /> New
          </button>
          <IconButton
            label="Save project (Ctrl+S)"
            onClick={() => void saveFile(true)}
            disabled={!view || busy}
          >
            <Save size={16} />
          </IconButton>
          <IconButton
            label="Export Style (Ctrl+Shift+S)"
            onClick={() => void saveFile(false)}
            disabled={!view || busy}
          >
            <Download size={16} />
          </IconButton>
          <IconButton
            label="Export section notes CSV"
            onClick={() => void exportCsv()}
            disabled={!view || busy}
          >
            <FileDown size={16} />
          </IconButton>
          <IconButton
            label="Export section JSON snapshot"
            onClick={() => void exportJson()}
            disabled={!view || busy}
          >
            <FileJson size={16} />
          </IconButton>
          <span className="divider" />
          <IconButton
            label="Undo (Ctrl+Z)"
            onClick={() => void state.history(false)}
            disabled={!view?.canUndo || editingDisabled}
          >
            <Undo2 size={16} />
          </IconButton>
          <IconButton
            label="Redo (Ctrl+Shift+Z)"
            onClick={() => void state.history(true)}
            disabled={!view?.canRedo || editingDisabled}
          >
            <Redo2 size={16} />
          </IconButton>
        </div>
        <div className="transport">
          <button
            type="button"
            disabled={!section || capturing || busy || transport.playing}
            onClick={() => setLivePreviewOpen(true)}
          >
            Live Style preview
          </button>
          <IconButton
            label="Stop / release notes (Escape)"
            onClick={() => void stop()}
            disabled={!transport.playing}
          >
            <Square size={14} />
          </IconButton>
          <IconButton
            label="Play source notes (Space)"
            onClick={() => void play()}
            disabled={!view || busy}
            active={transport.playing}
          >
            <Play size={18} fill="currentColor" />
          </IconButton>
          <IconButton
            label="Test note (audition)"
            onClick={() => void audition()}
            disabled={busy}
          >
            <Bell size={15} />
          </IconButton>
          <IconButton
            label="MIDI panic / all notes off"
            onClick={() => void panic()}
            disabled={busy}
          >
            <OctagonAlert size={15} />
          </IconButton>
          <IconButton
            label="Loop section"
            onClick={() => setLoop(!loop)}
            active={loop}
          >
            <Repeat2 size={16} />
          </IconButton>
          <div className="time-readout">
            {Math.floor(transport.elapsedMs / 60000)
              .toString()
              .padStart(2, "0")}
            :
            {Math.floor((transport.elapsedMs / 1000) % 60)
              .toString()
              .padStart(2, "0")}
            .
            <span>
              {(transport.elapsedMs % 1000).toString().padStart(3, "0")}
            </span>
          </div>
          <label className="tempo" title="Edit tempo">
            <input
              aria-label="Tempo BPM"
              type="number"
              min={20}
              max={400}
              step={0.01}
              disabled={!view || editingDisabled}
              key={view ? `t-${view.revision}-${view.tempo}` : "t"}
              defaultValue={view ? Number(view.tempo.toFixed(2)) : undefined}
              onBlur={(e) => {
                if (!view) return;
                const n = Number(e.target.value);
                if (
                  Number.isFinite(n) &&
                  n >= 20 &&
                  n <= 400 &&
                  Math.abs(n - view.tempo) > 0.001
                ) {
                  void applyTempoMeter(n, view.numerator, view.denominator);
                } else {
                  e.target.value = String(Number(view.tempo.toFixed(2)));
                }
              }}
            />
            <span>BPM</span>
          </label>
          <div className="meter meter-edit">
            <input
              aria-label="Meter numerator"
              type="number"
              min={1}
              max={16}
              disabled={!view || editingDisabled}
              key={view ? `n-${view.revision}-${view.numerator}` : "n"}
              defaultValue={view?.numerator}
              onBlur={(e) => {
                if (!view) return;
                const n = Number(e.target.value);
                if (
                  Number.isSafeInteger(n) &&
                  n >= 1 &&
                  n <= 16 &&
                  n !== view.numerator
                ) {
                  void applyTempoMeter(view.tempo, n, view.denominator);
                } else {
                  e.target.value = String(view.numerator);
                }
              }}
            />
            <span>/</span>
            <select
              aria-label="Meter denominator"
              disabled={!view || editingDisabled}
              value={view?.denominator ?? 4}
              onChange={(e) => {
                if (!view) return;
                const d = Number(e.target.value);
                if (d !== view.denominator) {
                  void applyTempoMeter(view.tempo, view.numerator, d);
                }
              }}
            >
              {[1, 2, 4, 8, 16].map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </div>
        </div>
        <button
          className="output-select"
          onClick={() => {
            setDeviceOpen(true);
            void scan();
          }}
        >
          <span className={`connection-dot ${output ? "connected" : ""}`} />
          {output?.name ?? "Select MIDI output"}
          <ChevronRight size={13} />
        </button>
      </div>
      {error && (
        <div className="error" role="alert">
          <span>{error}</span>
          <IconButton label="Dismiss error" onClick={state.clearError}>
            <X size={15} />
          </IconButton>
        </div>
      )}
      {!view ? (
        <main className="empty">
          <div className="empty-score">
            <Music2 size={45} />
            <div className="empty-staff" />
          </div>
          <p className="eyebrow">YOUR NEXT ARRANGEMENT</p>
          <h1>Give every part its place.</h1>
          <p>
            Open a Yamaha Style, or compose a blank project from a reference
            scaffold with cleared notes.
          </p>
          <div className="empty-actions">
            <button
              className="primary"
              onClick={() => void openFile()}
              disabled={busy}
            >
              <FolderOpen size={17} />
              {busy ? "Reading Style..." : "Open Yamaha Style"}
            </button>
            <button
              className="secondary"
              onClick={() => void openComposer()}
              disabled={busy}
            >
              <FilePlus2 size={17} />
              New blank Style
            </button>
          </div>
          <span className="empty-hint">
            STY · PRS · SST · PST · FPS · MIDI · GSS
          </span>
          {recent.length > 0 && (
            <section className="recent-panel" aria-label="Recent files">
              <h2 className="eyebrow">Recent</h2>
              <ul className="recent-list">
                {recent.map((r) => (
                  <li key={r.entry.id}>
                    <button
                      type="button"
                      className={r.available ? "" : "missing"}
                      disabled={busy || !r.available}
                      title={r.entry.path}
                      onClick={() => void openPath(r.entry.path)}
                    >
                      <strong>{r.entry.name}</strong>
                      <small>
                        {r.entry.kind === "project" ? "Project" : "Style"}
                        {r.available ? "" : " · unavailable"}
                      </small>
                    </button>
                    <IconButton
                      label={`Remove ${r.entry.name} from recent`}
                      onClick={() => void removeRecent(r.entry.id)}
                    >
                      <X size={14} />
                    </IconButton>
                  </li>
                ))}
              </ul>
            </section>
          )}
          <div className="empty-details">
            <span>Source timing preserved</span>
            <span>Native MIDI playback</span>
            <span>Independent Rust engine</span>
          </div>
        </main>
      ) : (
        <>
          <section className="sections-strip">
            <div className="section-heading">
              <span className="eyebrow">STYLE SECTIONS</span>
              <span>
                {view.format} <i>·</i> {view.ppq} PPQN
              </span>
            </div>
            <nav aria-label="Style sections">
              {view.sections.map((s) => (
                <button
                  key={s.id}
                  onClick={() => {
                    if (transport.playing) void stop();
                    state.section(s.id);
                  }}
                  className={`section-chip ${s.id === sectionId ? "selected" : ""}`}
                >
                  <span>{s.label}</span>
                  <small>
                    {(
                      (s.end - s.start) /
                      (((view.ppq * 4) / view.denominator) * view.numerator)
                    ).toLocaleString(undefined, {
                      maximumFractionDigits: 1,
                    })}{" "}
                    bars
                  </small>
                </button>
              ))}
            </nav>
          </section>
          <main className="workspace">
            <aside className="tracks">
              <div className="panel-heading">
                <span>SOURCE CHANNELS</span>
                <small>{sources.length}</small>
              </div>
              <div className="track-list">
                {sources.map((ch) => {
                  const route = view.routes.find(
                    (r) =>
                      r.source === ch &&
                      r.sections.includes(section?.label ?? ""),
                  );
                  return (
                    <button
                      key={ch}
                      className={`track ${ch === channel ? "selected" : ""}`}
                      onClick={() => state.setChannel(ch)}
                    >
                      <span
                        className="track-stripe"
                        style={{ background: colors[(ch - 9 + 16) % 8] }}
                      />
                      <span className="channel-number">
                        {String(ch).padStart(2, "0")}
                      </span>
                      <span className="track-content">
                        <strong>{role(ch)}</strong>
                        <small>
                          {sectionNotes.filter((n) => n.channel === ch).length}{" "}
                          notes
                          {route ? ` · → ch ${route.destination}` : ""}
                        </small>
                      </span>
                      <span className="track-indicator">
                        {muted.includes(ch)
                          ? "M"
                          : solo.includes(ch)
                            ? "S"
                            : ""}
                      </span>
                    </button>
                  );
                })}
              </div>
              <div className="track-summary">
                <span>
                  {sectionNotes.length.toLocaleString()} notes in section
                </span>
                <small>Source channels retain original routing.</small>
              </div>
            </aside>
            <section className="editor">
              <div className="editor-heading">
                <div className="view-tabs">
                  <button
                    className={tab === "notes" ? "active" : ""}
                    onClick={() => setTab("notes")}
                  >
                    <Music2 size={15} />
                    Piano roll
                  </button>
                  <button
                    className={tab === "mixer" ? "active" : ""}
                    onClick={() => setTab("mixer")}
                  >
                    <SlidersHorizontal size={15} />
                    Mixer
                  </button>
                </div>
                <span className="selection-caption">
                  {section?.label} <i>/</i> {role(channel)}
                </span>
              </div>
              {tab === "notes" ? (
                <>
                  <div className="edit-toolbar">
                    <div className="tool-group">
                      <IconButton
                        label="Select notes"
                        onClick={() => {
                          useStudio.setState({ tool: "select" });
                          void persistPrefs({ tool: "select" });
                        }}
                        active={tool === "select"}
                      >
                        <MousePointer2 size={15} />
                      </IconButton>
                      <IconButton
                        label="Draw notes"
                        onClick={() => {
                          useStudio.setState({ tool: "draw" });
                          void persistPrefs({ tool: "draw" });
                        }}
                        active={tool === "draw"}
                      >
                        <Pencil size={15} />
                      </IconButton>
                    </div>
                    <label className="inline-field">
                      Snap{" "}
                      <select
                        aria-label="Snap division"
                        value={snap}
                        onChange={(e) => {
                          const next = +e.target.value;
                          useStudio.setState({ snap: next });
                          void persistPrefs({ snap: next });
                        }}
                      >
                        {[4, 8, 16, 32, 64].map((n) => (
                          <option key={n} value={n}>
                            1/{n}
                          </option>
                        ))}
                      </select>
                    </label>
                    <button
                      disabled={!selected.length || editingDisabled}
                      onClick={() =>
                        void state.edit({
                          type: "quantize",
                          section: sectionId,
                          ids: selected,
                          step: (view.ppq * 4) / snap,
                        })
                      }
                    >
                      <Scissors size={14} />
                      Quantize
                    </button>
                    <span className="spacer" />
                    <IconButton
                      label="Zoom out"
                      onClick={() => {
                        const next = Math.max(0.125, zoom / 1.5);
                        useStudio.setState({ zoom: next });
                        void persistPrefs({ zoom: next });
                      }}
                    >
                      <ZoomOut size={15} />
                    </IconButton>
                    <span className="zoom-label">
                      {Math.round(zoom * 100)}%
                    </span>
                    <IconButton
                      label="Zoom in"
                      onClick={() => {
                        const next = Math.min(8, zoom * 1.5);
                        useStudio.setState({ zoom: next });
                        void persistPrefs({ zoom: next });
                      }}
                    >
                      <ZoomIn size={15} />
                    </IconButton>
                  </div>
                  <div
                    className={`canvas-wrap ${editingDisabled ? "locked" : ""}`}
                  >
                    <PianoRoll />
                  </div>
                  <div className="editor-footnote">
                    <span>
                      Double-click to draw · Shift to select · Drag edge to
                      resize
                    </span>
                    <span>
                      {selected.length
                        ? `${selected.length} selected`
                        : `${sectionNotes.filter((n) => n.channel === channel).length} notes`}
                    </span>
                  </div>
                </>
              ) : (
                <div className="mixer">
                  <p className="mixer-note">
                    Controllers are written at section start. Mute and solo
                    affect source playback.
                  </p>
                  <div className="mixer-strips">
                    {sources.map((ch) => (
                      <div className="mixer-strip" key={ch}>
                        <div
                          className="mixer-track-color"
                          style={{ background: colors[(ch - 9 + 16) % 8] }}
                        />
                        <strong>{role(ch)}</strong>
                        <span>CH {ch}</span>
                        <div className="mute-solo">
                          <button
                            className={muted.includes(ch) ? "active" : ""}
                            onClick={() =>
                              setMuted((a) =>
                                a.includes(ch)
                                  ? a.filter((x) => x !== ch)
                                  : [...a, ch],
                              )
                            }
                          >
                            M
                          </button>
                          <button
                            className={solo.includes(ch) ? "active" : ""}
                            onClick={() =>
                              setSolo((a) =>
                                a.includes(ch)
                                  ? a.filter((x) => x !== ch)
                                  : [...a, ch],
                              )
                            }
                          >
                            S
                          </button>
                        </div>
                        <fieldset disabled={editingDisabled}>
                          <Slider
                            label={`Ch ${ch} volume`}
                            value={valueAt(ch, 0xb0, 7, 100)}
                            onCommit={(n) => cc(ch, 7, n)}
                          />
                          <Slider
                            label={`Ch ${ch} pan`}
                            value={valueAt(ch, 0xb0, 10, 64)}
                            onCommit={(n) => cc(ch, 10, n)}
                          />
                          <Slider
                            label={`Ch ${ch} expression`}
                            value={valueAt(ch, 0xb0, 11, 127)}
                            onCommit={(n) => cc(ch, 11, n)}
                          />
                          <Slider
                            label={`Ch ${ch} reverb send`}
                            value={valueAt(ch, 0xb0, 91, 40)}
                            onCommit={(n) => cc(ch, 91, n)}
                          />
                          <Slider
                            label={`Ch ${ch} chorus send`}
                            value={valueAt(ch, 0xb0, 93, 0)}
                            onCommit={(n) => cc(ch, 93, n)}
                          />
                        </fieldset>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </section>
            <aside className="inspector">
              <div className="panel-heading">
                <span>INSPECTOR</span>
                <SlidersHorizontal size={14} />
              </div>
              <div className="inspector-tabs">
                {(["note", "casm", "events"] as const).map((t) => (
                  <button
                    key={t}
                    className={inspector === t ? "active" : ""}
                    onClick={() => setInspector(t)}
                  >
                    {t === "casm" ? "CASM" : t === "note" ? "Notes" : "Events"}
                  </button>
                ))}
              </div>
              <fieldset disabled={editingDisabled} className="inspector-body">
                {inspector === "note" ? (
                  <>
                    <div className="note-navigation">
                      <IconButton
                        label="Previous note"
                        disabled={noteIndex <= 0}
                        onClick={() =>
                          state.select([channelNotes[noteIndex - 1].id])
                        }
                      >
                        <ChevronLeft size={14} />
                      </IconButton>
                      <span>
                        {noteIndex < 0
                          ? "No selection"
                          : `${noteIndex + 1} of ${channelNotes.length}`}
                      </span>
                      <IconButton
                        label="Next note"
                        disabled={
                          !channelNotes.length ||
                          noteIndex >= channelNotes.length - 1
                        }
                        onClick={() =>
                          state.select([channelNotes[noteIndex + 1].id])
                        }
                      >
                        <ChevronRight size={14} />
                      </IconButton>
                    </div>
                    <h3>{note ? pitchName(note.pitch) : "Note selection"}</h3>
                    <p className="muted">
                      {selected.length
                        ? `${selected.length} selected · source ch ${channel}`
                        : "Select a note in the piano roll to inspect its timing and velocity."}
                    </p>
                    {note && section && (
                      <>
                        <NumberField
                          label="Pitch"
                          value={note.pitch}
                          onCommit={(n) =>
                            void state.edit({
                              type: "updateNotes",
                              section: sectionId,
                              ids: selected,
                              deltaPitch: n - note.pitch,
                            })
                          }
                        />
                        <NumberField
                          label="Start tick (section)"
                          value={note.start - section.start}
                          max={section.end - section.start - 1}
                          onCommit={(n) =>
                            void state.edit({
                              type: "updateNotes",
                              section: sectionId,
                              ids: selected,
                              deltaTick: n - (note.start - section.start),
                            })
                          }
                        />
                        <NumberField
                          label="Duration ticks"
                          value={note.duration}
                          min={1}
                          max={section.end - section.start}
                          onCommit={(n) =>
                            void state.edit({
                              type: "updateNotes",
                              section: sectionId,
                              ids: selected,
                              duration: n,
                            })
                          }
                        />
                        <NumberField
                          label="Velocity"
                          value={note.velocity}
                          min={1}
                          onCommit={(n) =>
                            void state.edit({
                              type: "updateNotes",
                              section: sectionId,
                              ids: selected,
                              velocity: n,
                            })
                          }
                        />
                        <button
                          className="danger-link"
                          onClick={() =>
                            void state.edit({
                              type: "deleteNotes",
                              ids: selected,
                            })
                          }
                        >
                          Delete {selected.length === 1 ? "note" : "selection"}
                        </button>
                      </>
                    )}
                    <div className="inspector-divider" />
                    <h3>Voice bank</h3>
                    {section && (
                      <VoiceName
                        section={section}
                        channel={channel}
                        onOpen={() => setVoiceOpen(true)}
                      />
                    )}
                    {section && (channel === 9 || channel === 10) && (
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => setDrumsOpen(true)}
                      >
                        Drum kits (Ch 9–10)
                      </button>
                    )}
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => setEffectsOpen(true)}
                    >
                      Effect types
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => setExchangeOpen(true)}
                      disabled={!section}
                    >
                      MIDI exchange
                    </button>
                    <button
                      type="button"
                      className="secondary"
                      onClick={() => setChordPreviewOpen(true)}
                      disabled={!section}
                    >
                      Chord preview
                    </button>
                    <p className="muted">
                      Numeric MIDI address at section start. Drum kits use the
                      imported Data List DrumKit category — not SysEx Drum
                      Setup. Reverb/chorus Apply sends verified system-effect
                      SysEx.
                    </p>
                    <NumberField
                      label="Bank MSB · CC 0"
                      value={valueAt(channel, 0xb0, 0)}
                      onCommit={(n) => cc(channel, 0, n)}
                    />
                    <NumberField
                      label="Bank LSB · CC 32"
                      value={valueAt(channel, 0xb0, 32)}
                      onCommit={(n) => cc(channel, 32, n)}
                    />
                    <NumberField
                      label="Program · 1-128"
                      value={valueAt(channel, 0xc0, undefined, 0) + 1}
                      min={1}
                      max={128}
                      onCommit={(n) =>
                        setDiscrete(channel, [0xc0 | (channel - 1), n - 1])
                      }
                    />
                  </>
                ) : inspector === "casm" ? (
                  <CasmInspector
                    routes={view.routes.filter(
                      (r) =>
                        r.source === channel &&
                        r.sections.includes(section?.label ?? ""),
                    )}
                    onEditSources={setSourcesRoute}
                  />
                ) : (
                  <EventInspector
                    events={view.events.filter(
                      (e) =>
                        section &&
                        e.tick >= section.start &&
                        e.tick < section.end &&
                        (e.data[0] & 15) + 1 === channel &&
                        e.data[0] >= 0xb0 &&
                        e.data[0] < 0xf0,
                    )}
                    sectionId={sectionId}
                    channel={channel}
                    start={section?.start ?? 0}
                    end={section?.end ?? 1}
                  />
                )}
              </fieldset>
              <div className="integrity">
                <span className="connection-dot connected" />
                <div>
                  Source precision
                  <small>{view.ppq} ticks per quarter note</small>
                </div>
              </div>
            </aside>
          </main>
        </>
      )}
      <footer className="statusbar">
        <span className={`connection-dot ${busy ? "" : "connected"}`} />
        <span>
          {busy
            ? "Processing..."
            : transport.playing
              ? "Playing source notes"
              : state.message}
        </span>
        {initialEffects?.view === view &&
          initialEffects?.output === output &&
          initialEffects && <span>· Initial Genos effects armed</span>}
        <span className="spacer" />
        {view && (
          <>
            <button
              className="status-action"
              onClick={() => setDiagnostics(true)}
            >
              {view.warnings.length
                ? `${view.warnings.length} diagnostic(s)`
                : "Structure validated"}
            </button>
            <i>·</i>
            <span>{view.notes.length.toLocaleString()} notes</span>
            <i>·</i>
            <span>
              {selectedNotes.length
                ? "Selection active"
                : `${view.chunks.length} chunks preserved`}
            </span>
          </>
        )}
        <span className="native-label">RUST ENGINE</span>
      </footer>
      {recovery && (
        <div className="modal-backdrop">
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="recovery-title"
          >
            <h2 id="recovery-title">Restore your last session?</h2>
            <p>
              An autosaved Style contains unsaved edits. Restore it before
              opening another file.
            </p>
            <div className="modal-actions">
              <button
                onClick={() => {
                  void invoke("discard_recovery")
                    .then(() => setRecovery(false))
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                Discard autosave
              </button>
              <button
                className="primary"
                onClick={() => {
                  void invoke<View>("restore_recovery")
                    .then((view) => {
                      useStudio.setState({
                        view,
                        sectionId: view.sections[0].id,
                        selected: [],
                        message: "Autosaved edits restored",
                      });
                      setRecovery(false);
                    })
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                Restore session
              </button>
            </div>
          </section>
        </div>
      )}
      {diagnostics && view && (
        <div className="modal-backdrop" onClick={() => setDiagnostics(false)}>
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="diagnostic-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-title">
              <h2 id="diagnostic-title">File integrity</h2>
              <IconButton
                label="Close diagnostics"
                onClick={() => setDiagnostics(false)}
              >
                <X size={18} />
              </IconButton>
            </div>
            <p>
              {view.format} · {view.ppq} PPQN ·{" "}
              {view.notes.length.toLocaleString()} paired notes
            </p>
            {view.chunks.map((c, i) => (
              <div className="device-row" key={`${c.tag}-${i}`}>
                <code>{c.tag}</code>
                <small>{c.bytes.toLocaleString()} bytes</small>
              </div>
            ))}
            <h3>Diagnostics</h3>
            {view.warnings.length ? (
              view.warnings.map((w) => (
                <p key={w} className="muted">
                  {w}
                </p>
              ))
            ) : (
              <p className="muted">
                No structural warnings. Hardware acceptance is a separate check.
              </p>
            )}
            <p className="muted">
              Exports require a new destination. Untouched chunks and source
              timing are retained; modified tracks are serialized and validated
              before writing.
            </p>
          </section>
        </div>
      )}
      {voiceOpen && section && (
        <VoiceBrowser
          key={`${section.id}-${channel}`}
          section={section}
          channel={channel}
          onClose={() => setVoiceOpen(false)}
        />
      )}
      {drumsOpen && section && (
        <DrumKits
          key={`drums-${section.id}-${channel}`}
          section={section}
          channel={channel}
          output={output}
          onClose={() => setDrumsOpen(false)}
        />
      )}
      {effectsOpen && (
        <EffectBrowser output={output} onClose={() => setEffectsOpen(false)} />
      )}
      {livePreviewOpen && section && view && (
        <LivePreview
          key={`live-${section.id}`}
          view={view}
          section={section}
          output={output}
          transport={transport}
          onTransport={setTransport}
          onClose={() => setLivePreviewOpen(false)}
        />
      )}
      {chordPreviewOpen && section && (
        <ChordPreview
          key={`chord-${section.id}-${channel}`}
          section={section}
          channel={channel}
          onClose={() => setChordPreviewOpen(false)}
        />
      )}
      {exchangeOpen && section && (
        <MidiExchange
          key={`${section.id}-${channel}`}
          section={section}
          channel={channel}
          onClose={() => setExchangeOpen(false)}
        />
      )}
      {sourcesRoute && section && (
        <SourceTracks
          route={sourcesRoute}
          section={section}
          onClose={() => setSourcesRoute(null)}
        />
      )}
      {composerOpen && (
        <div className="modal-backdrop" onClick={() => setComposerOpen(false)}>
          <section
            className="modal composer-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="composer-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-title">
              <h2 id="composer-title">Style Composer</h2>
              <IconButton
                label="Close Style Composer"
                disabled={busy}
                onClick={() => setComposerOpen(false)}
              >
                <X size={18} />
              </IconButton>
            </div>
            <p>
              Create a blank SFF2 project from a reference Style scaffold. Notes
              are cleared; section layout, CASM and controllers stay with the
              reference so export remains possible (EV-0142 / EV-0171).
            </p>
            <label className="field">
              <span>Project name</span>
              <input
                aria-label="Project name"
                value={composer.name}
                onChange={(e) =>
                  setComposer((c) => ({ ...c, name: e.target.value }))
                }
              />
            </label>
            <div className="composer-grid">
              <label className="field">
                <span>Tempo · BPM</span>
                <input
                  aria-label="Tempo BPM"
                  type="number"
                  min={20}
                  max={400}
                  value={composer.tempoBpm}
                  onChange={(e) =>
                    setComposer((c) => ({
                      ...c,
                      tempoBpm: Number(e.target.value),
                    }))
                  }
                />
              </label>
              <label className="field">
                <span>Meter</span>
                <div className="meter-inputs">
                  <input
                    aria-label="Meter numerator"
                    type="number"
                    min={1}
                    max={16}
                    value={composer.numerator}
                    onChange={(e) =>
                      setComposer((c) => ({
                        ...c,
                        numerator: Number(e.target.value),
                      }))
                    }
                  />
                  <span>/</span>
                  <select
                    aria-label="Meter denominator"
                    value={composer.denominator}
                    onChange={(e) =>
                      setComposer((c) => ({
                        ...c,
                        denominator: Number(e.target.value),
                      }))
                    }
                  >
                    {[1, 2, 4, 8, 16].map((d) => (
                      <option key={d} value={d}>
                        {d}
                      </option>
                    ))}
                  </select>
                </div>
              </label>
              <label className="field">
                <span>Format</span>
                <select aria-label="Style format" value="SFF2" disabled>
                  <option value="SFF2">SFF2</option>
                </select>
              </label>
            </div>
            <label className="field">
              <span>Reference Style scaffold</span>
              <div className="reference-row">
                <input
                  aria-label="Reference Style path"
                  readOnly
                  value={composer.referencePath}
                  placeholder="Required · choose a Style file"
                />
                <button type="button" onClick={() => void pickReference()}>
                  Choose…
                </button>
              </div>
            </label>
            <p className="muted">
              Default blank layout without a scaffold cannot export STY. SFF1
              blank authoring remains blocked pending verified conversion.
            </p>
            <div className="modal-actions">
              <button
                type="button"
                disabled={busy}
                onClick={() => setComposerOpen(false)}
              >
                Cancel
              </button>
              <button
                className="primary"
                type="button"
                disabled={busy || !composer.referencePath}
                onClick={() => void createBlank()}
              >
                {busy ? "Creating…" : "Create blank Style"}
              </button>
            </div>
          </section>
        </div>
      )}
      {deviceOpen && (
        <div className="modal-backdrop" onClick={() => setDeviceOpen(false)}>
          <section
            className="modal midi-connections"
            role="dialog"
            aria-modal="true"
            aria-labelledby="device-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-title">
              <h2 id="device-title">MIDI connections</h2>
              <IconButton
                label="Close MIDI settings"
                onClick={() => setDeviceOpen(false)}
              >
                <X size={18} />
              </IconButton>
            </div>
            <p>
              Choose a destination for source-note playback. Opening a Style
              sends no MIDI.
            </p>
            <label className="field">
              <span>MIDI output</span>
              <select
                aria-label="MIDI output"
                value={output ? `${output.index}:${output.name}` : ""}
                onChange={(e) => {
                  const port = devices.outputs.find(
                    (p) => `${p.index}:${p.name}` === e.target.value,
                  );
                  setOutput(port ?? null);
                  setInitialEffects(null);
                  void persistPrefs({
                    midiOutputName: port?.name ?? null,
                    midiOutputIndex: port?.index ?? null,
                  });
                }}
              >
                <option value="">No output selected</option>
                {devices.outputs.map((p) => (
                  <option
                    key={`${p.index}:${p.name}`}
                    value={`${p.index}:${p.name}`}
                  >
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <InitialEffects
              view={view}
              output={output}
              selection={initialEffects}
              onChange={setInitialEffects}
              disabled={busy || transport.playing || capturing}
            />
            <h3>MIDI input monitor</h3>
            <p className="muted local-control-tip">
              Host listen needs Genos MIDI Local Control / MIDI Out → USB so key
              presses appear on an input such as Digital Workstation-1. Match
              Genos transmit settings (Local Control / MIDI Out) so notes reach
              USB. Monitor is display-only (F026 capture is separate). Zero
              events usually means Local Out is off, not a broken port list.
            </p>
            <label className="field">
              <span>Input port</span>
              <select
                aria-label="MIDI input"
                value={monitorName}
                onChange={(e) => setMonitorName(e.target.value)}
              >
                <option value="">No input selected</option>
                {devices.inputs.map((p) => (
                  <option key={`${p.index}:${p.name}`} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <div className="monitor-actions">
              <button
                type="button"
                disabled={!monitorName || monitoring}
                onClick={() => {
                  void invoke("start_monitor", { name: monitorName })
                    .then(() => {
                      setMonitoring(true);
                      setMonitorEvents([]);
                    })
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                Start monitor
              </button>
              <button
                type="button"
                disabled={!monitoring}
                onClick={() => {
                  void invoke("stop_monitor")
                    .then(() => setMonitoring(false))
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                Stop
              </button>
              <button
                type="button"
                onClick={() => {
                  void invoke("clear_monitor")
                    .then(() => setMonitorEvents([]))
                    .catch(() => undefined);
                }}
              >
                Clear
              </button>
            </div>
            <div className="monitor-log" aria-live="polite">
              {monitorEvents.length ? (
                monitorEvents.map((ev, i) => (
                  <div key={`${ev.stampMs}-${i}`}>
                    <code>{ev.stampMs}ms</code> {ev.summary}
                  </div>
                ))
              ) : (
                <p className="muted">
                  {monitoring
                    ? "Listening... play keys on Genos with MIDI Out enabled."
                    : "Select an input and start the monitor (display only)."}
                </p>
              )}
            </div>
            <h3>MIDI capture</h3>
            <p className="muted">
              Raw input capture with markers. Exports a Type-0 MIDI file only -
              does not build Styles or CASM (F027 remains gated).
            </p>
            <div className="monitor-actions">
              <button
                type="button"
                disabled={!monitorName || capturing}
                onClick={() => {
                  void invoke("start_capture", { name: monitorName })
                    .then(() => {
                      setCapturing(true);
                      setMonitoring(false);
                      setCaptureSnap(null);
                    })
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                <CircleDot size={14} /> Start capture
              </button>
              <button
                type="button"
                disabled={!capturing}
                onClick={() => {
                  void invoke("stop_capture")
                    .then(() => {
                      setCapturing(false);
                      return invoke<CaptureSnapshot>("capture_snapshot");
                    })
                    .then(setCaptureSnap)
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                Stop capture
              </button>
              <button
                type="button"
                disabled={!capturing}
                onClick={() => {
                  const label = window.prompt("Marker label", "Mark") ?? "";
                  if (!label.trim()) return;
                  void invoke("capture_marker", { label: label.trim() })
                    .then(() => invoke<CaptureSnapshot>("capture_snapshot"))
                    .then(setCaptureSnap)
                    .catch((e) => useStudio.setState({ error: String(e) }));
                }}
              >
                Add marker
              </button>
              <button
                type="button"
                disabled={
                  capturing ||
                  !(captureSnap?.eventCount || captureSnap?.markers.length)
                }
                onClick={() => {
                  void (async () => {
                    try {
                      const path = await save({
                        defaultPath: "capture.mid",
                        filters: [{ name: "MIDI", extensions: ["mid"] }],
                      });
                      if (!path) return;
                      await invoke("export_capture_midi", { path });
                      useStudio.setState({
                        message: "Capture exported as MIDI (not a Style)",
                      });
                    } catch (e) {
                      useStudio.setState({ error: String(e) });
                    }
                  })();
                }}
              >
                Export MIDI…
              </button>
              <button
                type="button"
                disabled={
                  capturing ||
                  !view ||
                  !section ||
                  busy ||
                  !captureSnap?.eventCount
                }
                onClick={() => void applyCapture()}
              >
                Apply notes to section
              </button>
              <button
                type="button"
                disabled={capturing}
                onClick={() => {
                  void invoke("clear_capture")
                    .then(() =>
                      setCaptureSnap({
                        recording: capturing,
                        portName: monitorName || null,
                        events: [],
                        markers: [],
                        eventCount: 0,
                        droppedEvents: 0,
                      }),
                    )
                    .catch(() => undefined);
                }}
              >
                Clear capture
              </button>
            </div>
            <div className="monitor-log" aria-live="polite">
              {captureSnap?.eventCount || captureSnap?.markers.length ? (
                <>
                  <div>
                    <code>{captureSnap.eventCount}</code> events
                    {captureSnap.markers.length
                      ? ` · ${captureSnap.markers.length} markers`
                      : ""}
                    {capturing ? " · recording" : ""}
                  </div>
                  {captureSnap.markers.slice(-8).map((m, i) => (
                    <div key={`m-${m.stampMs}-${i}`}>
                      <code>{m.stampMs}ms</code> ◆ {m.label}
                    </div>
                  ))}
                  {captureSnap.events
                    .slice(-20)
                    .reverse()
                    .map((ev, i) => (
                      <div key={`c-${ev.stampMs}-${i}`}>
                        <code>{ev.stampMs}ms</code> {ev.summary}
                      </div>
                    ))}
                </>
              ) : (
                <p className="muted">
                  {capturing
                    ? "Capturing... play the keyboard; add markers as needed."
                    : "Start capture on an exact input port when ready."}
                </p>
              )}
            </div>
            <p className="muted">
              Playback uses source channels. Monitor and capture share the input
              port (one at a time). Live Style preview applies supported Ctb2
              routes and a chord progression; its coverage report lists excluded
              settings.
            </p>
            <div className="modal-actions">
              <button onClick={() => void scan()}>
                <RefreshCw size={15} />
                Refresh devices
              </button>
              <button
                className="primary"
                onClick={() => {
                  void persistPrefs({});
                  setDeviceOpen(false);
                }}
              >
                Done
              </button>
            </div>
          </section>
        </div>
      )}
      {expansionOpen && (
        <ExpansionRecovery onClose={() => setExpansionOpen(false)} />
      )}
      {helpOpen && (
        <div className="modal-backdrop" onClick={() => setHelpOpen(false)}>
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="help-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-title">
              <h2 id="help-title">Help</h2>
              <IconButton label="Close Help" onClick={() => setHelpOpen(false)}>
                <X size={18} />
              </IconButton>
            </div>
            <p>
              GenosStyleStudio edits Yamaha Styles with source timing preserved.
              It is an independent editor - not Style File Maker.
            </p>
            <h3>Shortcuts</h3>
            <ul className="help-list">
              <li>
                <kbd>Ctrl</kbd>+<kbd>O</kbd> Open · <kbd>Ctrl</kbd>+<kbd>N</kbd>{" "}
                New blank · <kbd>Ctrl</kbd>+<kbd>S</kbd> Save project
              </li>
              <li>
                <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Y</kbd> Undo / Redo · Space
                Play/Stop when a section is open
              </li>
            </ul>
            <h3>Known limits</h3>
            <ul className="help-list">
              <li>
                Capture exports raw MIDI only - no automatic Style/CASM build.
              </li>
              <li>
                Chord preview is a manual visual study. Live Style preview adds
                supported Ctb2 routing and a chord progression.
                Yamaha-equivalent chord playback and SFF1 conversion remain
                unavailable. Voice and effect catalogs require the supported
                Genos v2.00 Data List. Live system effect changes are not saved
                in a Style.
              </li>
              <li>
                Hardware timing and Yamaha acceptance remain open release gates.
              </li>
            </ul>
            <div className="modal-actions">
              <button className="primary" onClick={() => setHelpOpen(false)}>
                Close
              </button>
            </div>
          </section>
        </div>
      )}
      {limitsOpen && (
        <div className="modal-backdrop" onClick={() => setLimitsOpen(false)}>
          <section
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="limits-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="modal-title">
              <h2 id="limits-title">Feature inventory</h2>
              <IconButton
                label="Close feature inventory"
                onClick={() => setLimitsOpen(false)}
              >
                <X size={18} />
              </IconButton>
            </div>
            <p>
              Baseline status for the 35 known REA surfaces. Blocked items stay
              gated - GenosStyleStudio does not invent Yamaha algorithms.
            </p>
            <ul className="help-list inventory-list">
              <li>
                <strong>Implemented (tested scope):</strong> F001-F015, F017
                named/numeric voice, F018 Genos v2.00 Data List import, F019
                DrumKit picker + supported Genos initial-header scalar editing,
                bounded ch9/10 audition, F020 mixer CC7/10/11/91/93, F021 Effect
                Type List + reverb/chorus live apply and initial-header file
                editing + opt-in initial type/Return playback, F022-F023, F024
                manual study and limited routed progression playback, F025
                play/audition, F026 capture+apply notes, F028 CSV/JSON, F029
                notes-only MIDI exchange, F031 Help/inventory, F032
                panic/release, F033 prefs/recovery, F007/F016 source+Ctb2 CASM.
              </li>
              <li>
                <strong>Partial (expansion packs):</strong> PPI/PQI section map +
                PACK MIDI voice map → voice picker; PPF extract + structural
                rewrite; VPRS header; SME VWST decrypt (needs offline SME seed)
                and YEM-import validation remain open.
              </li>
              <li>
                <strong>Gated - no invent:</strong> F019 sound remaps / advanced
                fields / Genos2 / native setup MIDI preview and
                hardware acceptance, F021 variation apply + Parameter List /
                detailed/later effect replay, F024 full Yamaha-equivalent
                playback / hardware retarget / expansion qualification, F027
                Style/analyze-build from capture (no Style builder oracle), F029
                DAW session round-trip, F030 SFF1 conversion (EV-0171 scaffold /
                conversion blocked).
              </li>
              <li>
                <strong>Excluded:</strong> F034 proprietary licensing.
              </li>
              <li>
                <strong>Hardware F035:</strong> Fresh 78 paired input notes,
                audible notes/Stop, source passage and dry/wet reverb contrast
                (direct and saved-file startup) confirmed on the connected
                Genos. Routed C–Am–F–G playback, edited notes/controllers, CASM
                source root and initial effect contrast are user-confirmed
                through the app, including a voice change and bounded
                RealDrumKit channel10/key42 volume/pan/pitch. USB Style loading
                was tested separately. Full Yamaha arranger equivalence, clock
                and latency qualification remain open.
              </li>
            </ul>
            <div className="modal-actions">
              <button className="primary" onClick={() => setLimitsOpen(false)}>
                Close
              </button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function CasmInspector({
  routes,
  onEditSources,
}: {
  routes: Route[];
  onEditSources: (route: Route) => void;
}) {
  const edit = useStudio((s) => s.edit);
  if (!routes.length)
    return (
      <>
        <h3>No editable route</h3>
        <p className="muted">
          This source has no verified 47-byte Ctb2 route in the current section.
          Original CASM data stays intact.
        </p>
      </>
    );
  return (
    <>
      {routes.map((r) => (
        <div key={r.id} className="casm-route">
          <h3>
            Source {r.source} → Ch {r.destination}
          </h3>
          <p className="muted">
            Ctb2 · {r.bytes.length} bytes
            {r.sections.length > 1
              ? ` · Shared by ${r.sections.join(", ")}`
              : ""}
          </p>
          <label className="field">
            <span>Source root</span>
            <select
              aria-label={`Route ${r.id} source root`}
              value={r.sourceRoot}
              onChange={(e) =>
                void edit({
                  type: "casm",
                  route: r.id,
                  field: "root",
                  value: +e.target.value,
                })
              }
            >
              {Array.from({ length: 12 }, (_, n) => (
                <option key={n} value={n}>
                  {pitchName(n).replace("-1", "")}
                </option>
              ))}
            </select>
          </label>
          <NumberField
            label="Source chord ID"
            value={r.sourceChord}
            max={33}
            onCommit={(value) =>
              void edit({ type: "casm", route: r.id, field: "chord", value })
            }
          />
          <label className="field">
            <span>Middle-region NTR</span>
            <select
              disabled={r.middleNtr > 1}
              aria-label={`Route ${r.id} NTR`}
              value={r.middleNtr}
              onChange={(e) =>
                void edit({
                  type: "casm",
                  route: r.id,
                  field: "ntr",
                  value: +e.target.value,
                })
              }
            >
              <option value="0">Root Trans</option>
              <option value="1">Root Fixed</option>
              {r.middleNtr > 1 && (
                <option value={r.middleNtr}>
                  Raw {r.middleNtr} · read only
                </option>
              )}
            </select>
          </label>
          <label className="field">
            <span>Middle-region NTT</span>
            <select
              disabled={r.middleNtr > 1}
              aria-label={`Route ${r.id} NTT`}
              value={r.middleNtt}
              onChange={(e) =>
                void edit({
                  type: "casm",
                  route: r.id,
                  field: "ntt",
                  value: +e.target.value,
                })
              }
            >
              <option value="0">Bypass</option>
              <option value="1">Melody</option>
              {r.middleNtt > 1 && (
                <option value={r.middleNtt}>
                  Raw {r.middleNtt} · preserved
                </option>
              )}
            </select>
          </label>
          <p className="muted">
            Low/high rules, chord masks, NTT Bass and extension bytes are
            preserved. Only tested fields are editable.
          </p>
          <button
            type="button"
            className="secondary"
            aria-label="Edit source tracks"
            onClick={() => onEditSources(r)}
          >
            Source tracks…
          </button>
          <details>
            <summary>Raw route bytes</summary>
            <code className="hex">
              {r.bytes.map((b) => b.toString(16).padStart(2, "0")).join(" ")}
            </code>
          </details>
        </div>
      ))}
    </>
  );
}
function EventInspector({
  events,
  sectionId,
  channel,
  start,
  end,
}: {
  events: MidiEvent[];
  sectionId: number;
  channel: number;
  start: number;
  end: number;
}) {
  const edit = useStudio((s) => s.edit);
  const [id, setId] = useState<number | null>(null),
    [kind, setKind] = useState("control");
  const event = events.find((e) => e.id === id);
  const status = event?.data[0] ?? 0;
  useEffect(() => setId(null), [sectionId, channel]);
  return (
    <>
      <h3>Discrete MIDI events</h3>
      <p className="muted">
        Source ch {channel} · {events.length} events
      </p>
      <div className="event-list">
        {events.map((e) => (
          <button
            key={e.id}
            className={id === e.id ? "selected" : ""}
            onClick={() => setId(e.id)}
          >
            <span>{e.tick - start}</span>
            <span>
              {e.data[0] >> 4 === 0xb
                ? `CC ${e.data[1]}`
                : e.data[0] >> 4 === 0xc
                  ? "Program"
                  : e.data[0] >> 4 === 0xe
                    ? "Bend"
                    : "Pressure"}
            </span>
            <strong>{e.data.slice(1).join(" / ")}</strong>
          </button>
        ))}
      </div>
      {event && (
        <>
          <NumberField
            label="Event tick (section)"
            value={event.tick - start}
            max={end - start - 1}
            onCommit={(n) =>
              void edit({
                type: "setEvent",
                id: event.id,
                tick: start + n,
                data: event.data,
              })
            }
          />
          {event.data.slice(1).map((v, i) => (
            <NumberField
              key={`${event.id}:${i}:${v}`}
              label={
                status >> 4 === 0xb
                  ? i === 0
                    ? "Controller"
                    : "Value"
                  : `Data byte ${i + 1}`
              }
              value={v}
              onCommit={(n) => {
                const data = [...event.data];
                data[i + 1] = n;
                void edit({
                  type: "setEvent",
                  id: event.id,
                  tick: event.tick,
                  data,
                });
              }}
            />
          ))}
          <button
            className="danger-link"
            onClick={() => void edit({ type: "deleteEvent", id: event.id })}
          >
            Delete event
          </button>
        </>
      )}
      <div className="inspector-divider" />
      <label className="field">
        <span>New event</span>
        <select
          aria-label="New event type"
          value={kind}
          onChange={(e) => setKind(e.target.value)}
        >
          <option value="control">Controller</option>
          <option value="program">Program change</option>
          <option value="bend">Pitch bend</option>
        </select>
      </label>
      <button
        onClick={() =>
          void edit({
            type: "addEvent",
            section: sectionId,
            track: 0,
            tick: start,
            data:
              kind === "control"
                ? [0xb0 | (channel - 1), 1, 0]
                : kind === "program"
                  ? [0xc0 | (channel - 1), 0]
                  : [0xe0 | (channel - 1), 0, 64],
          })
        }
      >
        Add event at section start
      </button>
    </>
  );
}
