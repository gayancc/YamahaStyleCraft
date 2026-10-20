import { trapModalFocus } from "./modal";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Section } from "./contracts";
import { useStudio } from "./store";
import { pitchName } from "./PianoRoll";

type Vocabulary = {
  roots: string[];
  qualities: string[];
  disclaimer: string;
};

type ChordInspect = {
  root: number;
  rootName: string;
  quality: string;
  alias: string | null;
  tones: number[];
  scale: number[];
  absoluteTones: number[];
  disclaimer: string;
};

type Remapped = { source: number; preview: number | null; silent: boolean };

type PreviewKind = "pitched" | "guitar" | "expansion";
const nttModes = [
  ["bypass", "Bypass (root shift only)"],
  ["chord", "Chord"],
  ["melody", "Melody"],
  ["melodic_minor", "Melodic minor"],
  ["melodic_minor_5th", "Melodic minor 5th"],
  ["harmonic_minor", "Harmonic minor"],
  ["harmonic_minor_5th", "Harmonic minor 5th"],
  ["natural_minor", "Natural minor"],
  ["natural_minor_5th", "Natural minor 5th"],
  ["dorian", "Dorian"],
  ["dorian_5th", "Dorian 5th"],
] as const;
type Ntt = (typeof nttModes)[number][0];

export function ChordPreview({
  section,
  channel,
  onClose,
}: {
  section: Section;
  channel: number;
  onClose: () => void;
}) {
  const { view } = useStudio();
  const [vocab, setVocab] = useState<Vocabulary | null>(null);
  const [root, setRoot] = useState(0);
  const [sourceRoot, setSourceRoot] = useState(0);
  const [sourceQuality, setSourceQuality] = useState("Major");
  const [quality, setQuality] = useState("Major");
  const [kind, setKind] = useState<PreviewKind>("pitched");
  const [ntt, setNtt] = useState<Ntt>("chord");
  const [ntr, setNtr] = useState<"root_trans" | "root_fixed">("root_trans");
  const [bass, setBass] = useState(-1);
  const [inspect, setInspect] = useState<ChordInspect | null>(null);
  const [remap, setRemap] = useState<Remapped[]>([]);
  const [error, setError] = useState<string | null>(null);
  const dialog = useRef<HTMLElement>(null);

  const sourceNotes = useMemo(() => {
    if (!view) return [] as number[];
    const pitches = view.notes
      .filter(
        (n) =>
          n.channel === channel &&
          n.start >= section.start &&
          n.start < section.end,
      )
      .map((n) => n.pitch);
    return [...new Set(pitches)].sort((a, b) => a - b);
  }, [view, section.start, section.end, channel]);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    let live = true;
    void invoke<Vocabulary>("chord_preview_vocabulary")
      .then((v) => {
        if (live) {
          setVocab(v);
          setError(null);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
      previous?.focus();
    };
  }, []);

  useEffect(() => {
    let live = true;
    void invoke<ChordInspect>("inspect_chord_preview", { root, quality })
      .then((v) => {
        if (live) {
          setInspect(v);
          setError(null);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [root, quality]);

  useEffect(() => {
    let live = true;
    if (!sourceNotes.length) {
      setRemap([]);
      return;
    }
    setRemap([]);
    void invoke<Remapped[]>("remap_chord_preview_notes", {
      notes: sourceNotes,
      sourceRoot,
      sourceQuality,
      targetRoot: root,
      targetQuality: quality,
      ntt,
      ntr,
      highKey: 11,
      kind: kind === "expansion" ? "guitar" : kind,
      bass,
      expansionLayout: kind === "expansion",
    })
      .then((v) => {
        if (live) {
          setRemap(v);
          setError(null);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [
    sourceNotes,
    sourceRoot,
    sourceQuality,
    root,
    quality,
    ntt,
    ntr,
    kind,
    bass,
  ]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <section
        className="modal voice-browser"
        role="dialog"
        aria-modal="true"
        aria-labelledby="chord-preview-title"
        ref={dialog}
        tabIndex={-1}
        onKeyDown={trapModalFocus}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-title">
          <h2 id="chord-preview-title">Chord preview</h2>
          <button
            type="button"
            aria-label="Close chord preview"
            title="Close"
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <div className="voice-browser-body">
          <p className="muted">
            Inspect an approximate chord conversion for this phrase. Not a
            Yamaha arranger emulator. Display-only (no MIDI audition).
          </p>
          <p className="muted">
            Manual conversion study for Ch {channel}. Set the phrase's source
            chord below. These settings do not read or change its CASM routes.
          </p>
          <div className="drum-preview-row">
            <label className="field">
              <span>Source root</span>
              <select
                aria-label="Source chord root"
                value={sourceRoot}
                onChange={(e) => setSourceRoot(Number(e.target.value))}
              >
                {(vocab?.roots ?? []).map((name, i) => (
                  <option key={name} value={i}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Source quality</span>
              <select
                aria-label="Source chord quality"
                value={sourceQuality}
                onChange={(e) => setSourceQuality(e.target.value)}
              >
                {(vocab?.qualities ?? ["Major"])
                  .concat(["MajorTriad", "m7(11)"])
                  .map((q) => (
                    <option key={q} value={q}>
                      {q}
                    </option>
                  ))}
              </select>
            </label>
            <label className="field">
              <span>Kind</span>
              <select
                aria-label="Preview kind"
                value={kind}
                onChange={(e) => setKind(e.target.value as PreviewKind)}
              >
                <option value="pitched">Pitched (NTT/NTR)</option>
                <option value="guitar">Guitar lanes</option>
                <option value="expansion">Expansion layout</option>
              </select>
            </label>
            <label className="field">
              <span>Root</span>
              <select
                aria-label="Preview chord root"
                value={root}
                onChange={(e) => setRoot(Number(e.target.value))}
              >
                {(vocab?.roots ?? []).map((name, i) => (
                  <option key={name} value={i}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Quality</span>
              <select
                aria-label="Preview chord quality"
                value={quality}
                onChange={(e) => setQuality(e.target.value)}
              >
                {(vocab?.qualities ?? ["Major"]).map((q) => (
                  <option key={q} value={q}>
                    {q}
                  </option>
                ))}
              </select>
            </label>
            {kind === "pitched" && (
              <>
                <label className="field">
                  <span>NTT (preview)</span>
                  <select
                    aria-label="Preview NTT mode"
                    value={ntt}
                    onChange={(e) => setNtt(e.target.value as Ntt)}
                  >
                    {nttModes.map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  <span>NTR (preview)</span>
                  <select
                    aria-label="Preview NTR mode"
                    value={ntr}
                    onChange={(e) =>
                      setNtr(e.target.value as "root_trans" | "root_fixed")
                    }
                  >
                    <option value="root_trans">Root Trans</option>
                    <option value="root_fixed">Root Fixed</option>
                  </select>
                </label>
              </>
            )}
            {(kind === "guitar" || kind === "expansion") && (
              <label className="field">
                <span>Bass pc</span>
                <select
                  aria-label="Preview guitar bass"
                  value={bass}
                  onChange={(e) => setBass(Number(e.target.value))}
                >
                  <option value={-1}>Root (default)</option>
                  {(vocab?.roots ?? []).map((name, i) => (
                    <option key={name} value={i}>
                      {name}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </div>
          {inspect && (
            <div className="voice-confirm" aria-label="Chord tone inspection">
              <p>
                <strong>
                  {inspect.rootName} {inspect.quality}
                </strong>
                {inspect.alias ? ` (${inspect.alias})` : ""}
              </p>
              <p className="muted">
                Tones pc [{inspect.tones.join(", ")}] · absolute [
                {inspect.absoluteTones.join(", ")}] · scale [
                {inspect.scale.join(", ")}]
              </p>
            </div>
          )}
          <div className="voice-results" aria-label="Remapped source pitches">
            {remap.length === 0 ? (
              <p className="muted">No notes on this source in the section.</p>
            ) : (
              remap.map((row) => (
                <div key={row.source}>
                  <strong>{pitchName(row.source)}</strong>
                  <span>
                    {row.silent || row.preview == null
                      ? " → silent in converted preview"
                      : ` → ${pitchName(row.preview)}`}
                  </span>
                </div>
              ))
            )}
          </div>
          {error && <p className="error">{error}</p>}
        </div>
        <div className="modal-actions">
          <button type="button" className="primary" onClick={onClose}>
            Close
          </button>
        </div>
      </section>
    </div>
  );
}
