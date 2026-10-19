import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Port, Section, TransportStatus, View } from "./contracts";
import { trapModalFocus } from "./modal";
import { useStudio } from "./store";

type Chord = { root: number; quality: string; bars: number; bass?: number };
type Report = {
  steps: { label: string; startMs: number; endMs: number; notes: number }[];
  routes: {
    source: number;
    destination: number;
    notes: number;
    drum: boolean;
    voice: { msb: number; lsb: number; program: number };
  }[];
  coverage: { area: string; status: string; detail: string; count: number }[];
  warnings: string[];
  durationMs: number;
  noteOns: number;
  changedPitches: number;
  setupPackets: number;
  timedPackets: number;
};
const roots = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];
const qualities = [
  "Major",
  "Minor",
  "7",
  "Maj7",
  "m7",
  "m7b5",
  "dim",
  "aug",
  "sus4",
  "sus2",
];
export function LivePreview({
  view,
  section,
  output,
  transport,
  onTransport,
  onClose,
}: {
  view: View;
  section: Section;
  output: Port | null;
  transport: TransportStatus;
  onTransport: (status: TransportStatus) => void;
  onClose: () => void;
}) {
  const docLabel = useStudio((s) => s.docLabel);
  const channels = [
    ...new Set(
      view.notes
        .filter(
          (n) => n.start < section.end && n.start + n.duration > section.start,
        )
        .map((n) => n.channel),
    ),
  ].sort((a, b) => a - b);
  const [sources, setSources] = useState(channels);
  const [chords, setChords] = useState<Chord[]>([
    { root: 0, quality: "Major", bars: 1 },
    { root: 9, quality: "Minor", bars: 1 },
    { root: 5, quality: "Major", bars: 1 },
    { root: 7, quality: "Major", bars: 1 },
  ]);
  const [effects, setEffects] = useState(false),
    [drums, setDrums] = useState(false),
    [looping, setLooping] = useState(false);
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{
    token: string;
    report: Report;
    identity: string;
    view: View;
  } | null>(null);
  const identity = JSON.stringify({
    section: section.id,
    sources,
    chords,
    effects,
    drums,
    output,
  });
  const current = useRef({ identity, view });
  current.current = { identity, view };
  const mounted = useRef(true),
    inFlight = useRef(false),
    dialog = useRef<HTMLElement>(null);
  useEffect(() => {
    mounted.current = true;
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.focus();
    return () => {
      mounted.current = false;
      previous?.focus();
    };
  }, []);
  const ready =
    preview?.identity === identity && preview.view === view ? preview : null;
  const run = async (action: () => Promise<void>) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      inFlight.current = false;
      if (mounted.current) setPending(false);
    }
  };
  const stop = async () => {
    await invoke("stop");
    onTransport({ ...transport, playing: false });
  };
  const close = () =>
    void run(async () => {
      await stop();
      onClose();
    });
  const check = () =>
    void run(async () => {
      setPreview(null);
      const result = await invoke<{ token: string; report: Report }>(
        "preview_live_style",
        {
          revision: view.revision,
          docLabel,
          config: { section: section.id, sources, chords, effects, drums },
        },
      );
      if (
        mounted.current &&
        current.current.identity === identity &&
        current.current.view === view
      )
        setPreview({ ...result, identity, view });
    });
  const play = () =>
    void run(async () => {
      if (!ready || !output) return;
      await invoke("play_live_style", {
        token: ready.token,
        index: output.index,
        name: output.name,
        looping,
      });
      onTransport({ playing: true, elapsedMs: 0, error: null });
    });
  const changeChord = (index: number, patch: Partial<Chord>) =>
    setChords((items) =>
      items.map((item, i) => (i === index ? { ...item, ...patch } : item)),
    );
  const locked = pending || transport.playing;
  const elapsed = ready ? transport.elapsedMs % ready.report.durationMs : 0;
  const active = transport.playing
    ? ready?.report.steps.find((s) => elapsed >= s.startMs && elapsed < s.endMs)
    : null;
  return (
    <div className="modal-backdrop">
      <section
        ref={dialog}
        tabIndex={-1}
        className="modal live-preview-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Live Style preview"
        onKeyDown={(e) => {
          trapModalFocus(e);
          if (e.key === "Escape") {
            e.preventDefault();
            if (!inFlight.current) close();
          }
        }}
      >
        <div className="modal-title">
          <h2>Live Style preview · {section.label}</h2>
          <button disabled={pending} onClick={close}>
            Close
          </button>
        </div>
        <p>
          Play this section through its Ctb2 routes and a chord progression.
          Conversion is approximate; check the coverage before Play. Closing
          stops playback. Slash bass affects only NTT Bass-enabled CASM regions;
          unchanged chords preserve held notes.
        </p>
        <p className="muted">
          Output: {output?.name ?? "Choose an output in MIDI Devices first"}.
          Initial effects change global keyboard settings. Setup is sent once
          before the progression.
        </p>
        <fieldset disabled={locked}>
          <legend>Source channels</legend>
          <div className="live-sources">
            {channels.map((ch) => (
              <label key={ch}>
                <input
                  type="checkbox"
                  aria-label={`Preview source ${ch}`}
                  checked={sources.includes(ch)}
                  onChange={(e) =>
                    setSources((items) =>
                      e.target.checked
                        ? [...items, ch].sort((a, b) => a - b)
                        : items.filter((c) => c !== ch),
                    )
                  }
                />{" "}
                {ch} →{" "}
                {view.routes
                  .filter(
                    (r) =>
                      r.source === ch && r.sections.includes(section.label),
                  )
                  .map((r) => r.destination)
                  .join(", ") || "no route"}
              </label>
            ))}
          </div>
        </fieldset>
        <fieldset disabled={locked}>
          <legend>Chord progression</legend>
          <div className="live-chords">
            {chords.map((c, i) => (
              <div key={i}>
                <span>{i + 1}.</span>
                <select
                  aria-label={`Chord ${i + 1} root`}
                  value={c.root}
                  onChange={(e) => changeChord(i, { root: +e.target.value })}
                >
                  {roots.map((r, n) => (
                    <option key={r} value={n}>
                      {r}
                    </option>
                  ))}
                </select>
                <select
                  aria-label={`Chord ${i + 1} quality`}
                  value={c.quality}
                  onChange={(e) => changeChord(i, { quality: e.target.value })}
                >
                  {qualities.map((q) => (
                    <option key={q}>{q}</option>
                  ))}
                </select>
                <label>
                  Slash bass{" "}
                  <select
                    aria-label={`Chord ${i + 1} bass`}
                    value={c.bass ?? ""}
                    onChange={(e) =>
                      changeChord(i, {
                        bass:
                          e.target.value === "" ? undefined : +e.target.value,
                      })
                    }
                  >
                    <option value="">Chord root</option>
                    {roots.map((r, n) => (
                      <option key={r} value={n}>
                        {r}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Bars{" "}
                  <input
                    aria-label={`Chord ${i + 1} bars`}
                    type="number"
                    min={1}
                    max={16}
                    value={c.bars}
                    onChange={(e) => changeChord(i, { bars: +e.target.value })}
                  />
                </label>
                <button
                  disabled={chords.length === 1}
                  onClick={() =>
                    setChords((items) => items.filter((_, n) => n !== i))
                  }
                  aria-label={`Remove chord ${i + 1}`}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
          <button
            disabled={chords.length >= 32}
            onClick={() =>
              setChords((items) => [
                ...items,
                { root: 0, quality: "Major", bars: 1 },
              ])
            }
          >
            Add chord
          </button>
        </fieldset>
        <fieldset disabled={locked}>
          <legend>Typed initial setup</legend>
          <label>
            <input
              type="checkbox"
              checked={effects}
              onChange={(e) => setEffects(e.target.checked)}
            />{" "}
            Initial reverb/chorus type and Return
          </label>{" "}
          <label>
            <input
              type="checkbox"
              checked={drums}
              onChange={(e) => setDrums(e.target.checked)}
            />{" "}
            Explicit original drum per-key parameters
          </label>
        </fieldset>
        <div className="modal-actions">
          <button disabled={locked || !sources.length} onClick={check}>
            Check live preview
          </button>
          <label>
            <input
              type="checkbox"
              disabled={locked}
              checked={looping}
              onChange={(e) => setLooping(e.target.checked)}
            />{" "}
            Loop progression
          </label>
          <button disabled={locked || !ready || !output} onClick={play}>
            Play progression
          </button>
          <button disabled={pending} onClick={() => void run(stop)}>
            Stop
          </button>
          <strong role="status">
            {pending
              ? "Working…"
              : active
                ? `Playing ${active.label}`
                : ready
                  ? "Preview ready"
                  : "Check required"}
          </strong>
        </div>
        {(error || transport.error) && (
          <p className="error" role="alert">
            {error || transport.error}
          </p>
        )}
        {ready && (
          <div className="live-report">
            <p>
              {ready.report.noteOns} note pairs · {ready.report.changedPitches}{" "}
              changed pitches · {(ready.report.durationMs / 1000).toFixed(1)}{" "}
              seconds · {ready.report.setupPackets} setup messages
            </p>
            <table>
              <thead>
                <tr>
                  <th>Area</th>
                  <th>Coverage</th>
                  <th>Detail</th>
                </tr>
              </thead>
              <tbody>
                {ready.report.coverage.map((c) => (
                  <tr key={c.area}>
                    <td>{c.area}</td>
                    <td>
                      {c.status} ({c.count})
                    </td>
                    <td>{c.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <ul>
              {ready.report.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          </div>
        )}
      </section>
    </div>
  );
}
