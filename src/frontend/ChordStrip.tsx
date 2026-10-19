import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Vocabulary = { roots: string[]; qualities: string[] };
type PreviewMode = "source" | "manual" | "midi";
type Recognized = { root: number; quality: string };

const PITCH_NAMES = [
  "C",
  "C♯",
  "D",
  "D♯",
  "E",
  "F",
  "F♯",
  "G",
  "G♯",
  "A",
  "A♯",
  "B",
];
function pitchLabel(n: number) {
  return `${PITCH_NAMES[n % 12]}${Math.floor(n / 12) - 1}`;
}

/**
 * Always-visible chord preview strip (SFM-045).
 * Modes: Source notes · Manual (root/type/bass) · MIDI LH (held notes via
 * input monitor with channel + left-hand upper-note split like SFM).
 */
export function ChordStrip({
  onOpenDetails,
  midiInputName,
  monitoring,
  onMonitoringChange,
}: {
  onOpenDetails: () => void;
  /** Selected MIDI input port name from the Recording/Monitor panel. */
  midiInputName?: string;
  monitoring?: boolean;
  onMonitoringChange?: (active: boolean) => void;
}) {
  const [vocab, setVocab] = useState<Vocabulary | null>(null);
  const [mode, setMode] = useState<PreviewMode>("source");
  const [root, setRoot] = useState(0);
  const [quality, setQuality] = useState("Major");
  const [bass, setBass] = useState(-1);
  const [held, setHeld] = useState<number[]>([]);
  const [midiStatus, setMidiStatus] = useState<string | null>(null);
  /** 0 = OMNI; 1–16 = MIDI channel filter (SFM chordPreviewChannel). */
  const [midiChannel, setMidiChannel] = useState(0);
  /** Inclusive LH upper pitch; SFM default 59 (B3). */
  const [lhSplit, setLhSplit] = useState(59);
  const prevChord = useRef<Recognized | null>(null);
  const startedByStrip = useRef(false);

  useEffect(() => {
    let live = true;
    void invoke<Vocabulary>("chord_preview_vocabulary")
      .then((v) => {
        if (live) setVocab(v);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  // Auto-start input monitor when entering MIDI LH mode.
  useEffect(() => {
    if (mode !== "midi") {
      if (startedByStrip.current) {
        startedByStrip.current = false;
        void invoke("stop_monitor")
          .then(() => onMonitoringChange?.(false))
          .catch(() => undefined);
      }
      setMidiStatus(null);
      setHeld([]);
      return;
    }
    if (!midiInputName) {
      setMidiStatus("Select a MIDI input in Recording / Monitor");
      return;
    }
    if (monitoring) {
      setMidiStatus(null);
      return;
    }
    let cancelled = false;
    void invoke("start_monitor", { name: midiInputName })
      .then(() => {
        if (cancelled) return;
        startedByStrip.current = true;
        onMonitoringChange?.(true);
        setMidiStatus(null);
      })
      .catch((e) => {
        if (!cancelled) setMidiStatus(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [mode, midiInputName, monitoring, onMonitoringChange]);

  // Poll held notes (channel + LH split) and recognize while in MIDI mode.
  useEffect(() => {
    if (mode !== "midi" || !monitoring) return;
    let live = true;
    const tick = () => {
      void invoke<number[]>("monitor_held_notes", {
        channel: midiChannel > 0 ? midiChannel : null,
        maxPitch: lhSplit,
      })
        .then(async (notes) => {
          if (!live) return;
          setHeld(notes);
          if (notes.length === 0) {
            prevChord.current = null;
            return;
          }
          const hit = await invoke<Recognized | null>("recognize_chord_preview", {
            notes,
          });
          if (!live || !hit) return;
          prevChord.current = hit;
          setRoot(hit.root);
          setQuality(hit.quality);
          const lowest = Math.min(...notes) % 12;
          setBass(lowest === hit.root ? -1 : lowest);
        })
        .catch(() => undefined);
    };
    tick();
    const id = window.setInterval(tick, 80);
    return () => {
      live = false;
      window.clearInterval(id);
    };
  }, [mode, monitoring, midiChannel, lhSplit]);

  const manual = mode === "manual";
  const midiLive = mode === "midi";
  const rootName = vocab?.roots?.[root] ?? `PC${root}`;

  return (
    <div className="chord-strip" aria-label="Chord preview">
      <span className="chord-strip-label" title="Chord preview">
        Chord
      </span>
      <select
        aria-label="Chord preview mode"
        value={mode}
        title={
          mode === "midi"
            ? "Recognize chords from held MIDI notes in the left-hand range"
            : mode === "manual"
              ? "Manual root/type/bass for conversion study"
              : "Source notes mode (SFM)"
        }
        onChange={(e) => setMode(e.target.value as PreviewMode)}
      >
        <option value="source">Source notes</option>
        <option value="manual">Manual</option>
        <option value="midi">MIDI LH</option>
      </select>
      <label>
        Root{" "}
        <select
          aria-label="Chord preview root"
          value={root}
          disabled={!manual}
          onChange={(e) => setRoot(Number(e.target.value))}
        >
          {(vocab?.roots ?? ["C"]).map((name, i) => (
            <option key={name} value={i}>
              {name}
            </option>
          ))}
        </select>
      </label>
      <label>
        Type{" "}
        <select
          aria-label="Chord preview type"
          value={quality}
          disabled={!manual}
          onChange={(e) => setQuality(e.target.value)}
        >
          {(vocab?.qualities ?? ["Major"]).map((q) => (
            <option key={q} value={q}>
              {q}
            </option>
          ))}
        </select>
      </label>
      <label>
        Bass{" "}
        <select
          aria-label="Chord preview bass"
          value={bass}
          disabled={!manual}
          onChange={(e) => setBass(Number(e.target.value))}
        >
          <option value={-1}>Chord root</option>
          {(vocab?.roots ?? []).map((name, i) => (
            <option key={`b-${name}`} value={i}>
              {name}
            </option>
          ))}
        </select>
      </label>
      {midiLive && (
        <>
          <label>
            MIDI ch{" "}
            <select
              aria-label="Chord preview MIDI channel"
              value={midiChannel}
              onChange={(e) => setMidiChannel(Number(e.target.value))}
            >
              <option value={0}>OMNI</option>
              {Array.from({ length: 16 }, (_, i) => i + 1).map((ch) => (
                <option key={ch} value={ch}>
                  {ch}
                </option>
              ))}
            </select>
          </label>
          <label title="Notes above this pitch are ignored (SFM left-hand upper note)">
            LH upper{" "}
            <select
              aria-label="Left-hand upper note"
              value={lhSplit}
              onChange={(e) => setLhSplit(Number(e.target.value))}
            >
              {Array.from({ length: 49 }, (_, i) => i + 36).map((n) => (
                <option key={n} value={n}>
                  {pitchLabel(n)} ({n})
                </option>
              ))}
            </select>
          </label>
          <span
            className="muted"
            title={
              midiStatus ??
              (held.length
                ? `Held: ${held.join(", ")}`
                : "Hold notes in the left-hand range")
            }
            aria-live="polite"
          >
            {midiStatus
              ? midiStatus
              : held.length
                ? `${rootName}${quality === "Major" ? "" : quality} · ${held.length} notes`
                : "Waiting for LH notes…"}
          </span>
        </>
      )}
      <button type="button" className="ghost" onClick={onOpenDetails}>
        Preview details
      </button>
    </div>
  );
}
