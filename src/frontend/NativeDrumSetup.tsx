import { askConfirm } from "./confirmDialog";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStudio } from "./store";
import { trapModalFocus } from "./modal";
import { pitchName } from "./PianoRoll";
import type { View } from "./contracts";

type Field = "volume" | "pan" | "pitch";
type Key = {
  note: number;
  used: boolean;
  volume: number | null;
  pan: number | null;
  pitch: number | null;
  fine: number | null;
};
export type NativeDrumContext = {
  kitName: string;
  header: {
    channel: number;
    setup: number;
    address: { msb: number; lsb: number; program: number };
    keys: Key[];
    parameterCount: number;
  };
};
const fields: { field: Field; label: string; min: number; max: number }[] = [
  { field: "volume", label: "Volume", min: 0, max: 127 },
  { field: "pan", label: "Pan", min: 1, max: 127 },
  { field: "pitch", label: "Pitch (semitones)", min: -64, max: 63 },
];
export function NativeDrumSetup({
  channel,
  output,
  onClose,
}: {
  channel: number;
  output?: { index: number; name: string } | null;
  onClose: () => void;
}) {
  const { view } = useStudio();
  const [original] = useState(view!);
  const [context, setContext] = useState<NativeDrumContext | null>(null);
  const [note, setNote] = useState(36),
    [draft, setDraft] = useState<Record<string, string>>({}),
    [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null),
    [pending, setPending] = useState(false);
  const dialog = useRef<HTMLElement>(null),
    working = useRef(false),
    close = useRef(onClose);
  close.current = onClose;
  const stale = view !== original;
  useEffect(() => {
    let live = true;
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
    void invoke<NativeDrumContext>("inspect_native_drums", {
      channel,
      revision: original.revision,
    })
      .then((value) => {
        if (live) {
          setContext(value);
          setNote(value.header.keys.find((k) => k.used)?.note ?? 36);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
      previous?.focus();
    };
  }, [channel, original]);
  const key = context?.header.keys.find((k) => k.note === note);
  const filteredKeys =
    context?.header.keys.filter((k) => {
      if (!search.trim()) return true;
      const q = search.trim().toLowerCase();
      return (
        String(k.note).includes(q) ||
        pitchName(k.note).toLowerCase().includes(q)
      );
    }) ?? [];
  const changes = Object.entries(draft)
    .filter(([, v]) => v.trim() !== "")
    .map(([id, value]) => {
      const [note, field] = id.split(":");
      return {
        note: Number(note),
        field: field as Field,
        value: Number(value),
      };
    })
    .filter(
      (c) =>
        context?.header.keys.find((k) => k.note === c.note)?.[c.field] !==
        c.value,
    );
  const audition = async () => {
    if (!output || working.current || pending || stale) return;
    if (note < 13 || note > 91) {
      setError("Audition is limited to drum notes 13–91.");
      return;
    }
    working.current = true;
    setPending(true);
    setError(null);
    try {
      const drumCh =
        context?.header.channel === 9 || context?.header.channel === 10
          ? context.header.channel
          : channel === 9 || channel === 10
            ? channel
            : 10;
      await invoke("preview_drum_note", {
        index: output.index,
        name: output.name,
        channel: drumCh,
        pitch: note,
        velocity: 90,
        holdMs: 200,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      setPending(false);
    }
  };
  const valid = changes.every((c) => {
    const f = fields.find((f) => f.field === c.field)!;
    return Number.isInteger(c.value) && c.value >= f.min && c.value <= f.max;
  });
  const dismiss = () => {
    if (!working.current) close.current();
  };
  const apply = async () => {
    if (
      !context ||
      !changes.length ||
      !valid ||
      stale ||
      working.current ||
      useStudio.getState().busy
    )
      return;
    if (useStudio.getState().view !== original) {
      setError("Document changed; reopen Drum Setup.");
      return;
    }
    working.current = true;
    setPending(true);
    setError(null);
    useStudio.setState({ busy: true });
    try {
      const next = await invoke<View>("apply_native_drums", {
        channel,
        revision: original.revision,
        address: context.header.address,
        changes,
      });
      useStudio.setState({
        view: next,
        message: "Genos Drum Setup applied · one undo step",
      });
      close.current();
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      setPending(false);
      useStudio.setState({ busy: false });
    }
  };
  const resetKeys = async (notes: number[]) => {
    if (!context || stale || working.current || useStudio.getState().busy) return;
    if (useStudio.getState().view !== original) {
      setError("Document changed; reopen Drum Setup.");
      return;
    }
    const scope = notes.length ? `MIDI ${notes.join(", ")}` : "all overridden keys";
    if (
      !(await askConfirm(
        `Reset drum scalars for ${scope} to kit defaults? One undo step.`,
        { title: "Reset drum scalars", confirmLabel: "Reset" },
      ))
    )
      return;
    if (useStudio.getState().view !== original) {
      setError("Document changed; reopen Drum Setup.");
      return;
    }
    working.current = true;
    setPending(true);
    setError(null);
    useStudio.setState({ busy: true });
    try {
      const next = await invoke<View>("reset_native_drums", {
        channel,
        revision: original.revision,
        address: context.header.address,
        notes,
      });
      useStudio.setState({
        view: next,
        message: notes.length
          ? `Drum key ${notes[0]} reset to kit defaults · one undo`
          : "All drum scalar overrides reset · one undo",
      });
      close.current();
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      setPending(false);
      useStudio.setState({ busy: false });
    }
  };
  return (
    <div
      className="modal-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
    >
      <section
        ref={dialog}
        tabIndex={-1}
        className="modal voice-browser"
        role="dialog"
        aria-modal="true"
        aria-labelledby="native-drums-title"
        onKeyDown={(e) => {
          trapModalFocus(e);
          if (e.key === "Escape") {
            e.stopPropagation();
            e.preventDefault();
            dismiss();
          }
        }}
      >
        <header className="modal-title">
          <h2 id="native-drums-title">Genos per-key Drum Setup</h2>
          <button disabled={pending} onClick={dismiss}>
            Close
          </button>
        </header>
        <div className="voice-browser-body">
          <p>
            Genos profile · initial Style header · channel {channel}. Changes
            apply to this rhythm source in every section and are saved in STY
            and GSS. Notes and velocities stay unchanged.
          </p>
          <p className="muted">
            Volume 0–127 · pan 1 left / 64 center / 127 right · pitch −64 to +63
            whole semitones. Blank fields keep the current setting. Undo
            restores the previous setup. No MIDI is sent by this editor. The
            app's current MIDI preview does not apply these settings; verify the
            exported Style on Genos.
          </p>
          <p className="muted">
            Only Type-0 SFF2 factory-kit headers and unchanged rhythm routing
            are supported. Sound replacement, advanced parameters, Genos2 and
            expansion kits are unavailable here. Hardware acceptance remains
            unqualified.
          </p>
          {context && (
            <>
              <p>
                {context.kitName} · MSB {context.header.address.msb} · LSB{" "}
                {context.header.address.lsb} · PC{" "}
                {context.header.address.program + 1} · setup{" "}
                {context.header.setup + 1}
              </p>
              <label className="field">
                Search drums
                <input
                  aria-label="Search drums"
                  type="search"
                  value={search}
                  disabled={pending || stale}
                  placeholder="Name or MIDI note…"
                  onChange={(e) => setSearch(e.target.value)}
                />
              </label>
              <label className="field">
                Drum key
                <select
                  aria-label="Native drum key"
                  value={note}
                  disabled={pending || stale || !filteredKeys.length}
                  onChange={(e) => setNote(Number(e.target.value))}
                >
                  {filteredKeys.map((k) => (
                    <option key={k.note} value={k.note}>
                      {pitchName(k.note)} · MIDI {k.note}
                      {k.used ? " · used" : ""}
                    </option>
                  ))}
                </select>
              </label>
              {!filteredKeys.length && (
                <p className="muted">No drum keys match that search.</p>
              )}
              <div className="drum-preview-row">
                {fields.map((f) => (
                  <label className="field" key={f.field}>
                    <span>{f.label}</span>
                    <input
                      aria-label={`Drum ${f.label}`}
                      type="number"
                      min={f.min}
                      max={f.max}
                      step={1}
                      value={draft[`${note}:${f.field}`] ?? ""}
                      placeholder={
                        key?.[f.field] == null
                          ? "Kit default"
                          : String(key[f.field])
                      }
                      disabled={
                        pending ||
                        stale ||
                        (f.field === "pitch" &&
                          key?.fine != null &&
                          key.fine !== 0)
                      }
                      onChange={(e) =>
                        setDraft({
                          ...draft,
                          [`${note}:${f.field}`]: e.target.value,
                        })
                      }
                    />
                    <small>
                      Current:{" "}
                      {key?.[f.field] == null
                        ? "kit default"
                        : f.field === "pan" && key.pan === 0
                          ? "random"
                          : key[f.field]}
                    </small>
                  </label>
                ))}
              </div>
              {key?.fine != null && key.fine !== 0 && (
                <p className="muted">
                  Pitch is read-only because this key has non-neutral fine
                  tuning ({key.fine} cents).
                </p>
              )}
              <p aria-label="Drum change summary">
                {changes.length} scalar change(s) · one undo step
              </p>
              <ul>
                {changes.map((c) => (
                  <li key={`${c.note}:${c.field}`}>
                    MIDI {c.note} · {c.field}: {c.value}
                  </li>
                ))}
              </ul>
            </>
          )}
          {!context && !error && (
            <p role="status">Checking initial drum header…</p>
          )}
          {stale && (
            <p className="error">
              Document changed. Close and reopen Drum Setup.
            </p>
          )}
          {!valid && (
            <p className="error">
              Enter whole numbers in the displayed ranges.
            </p>
          )}
          {error && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
        </div>
        <footer className="modal-actions">
          <button disabled={pending} onClick={dismiss}>
            Cancel
          </button>
          <button
            type="button"
            disabled={!context || !output || pending || stale}
            title={
              output
                ? "Audition selected drum key on MIDI output"
                : "Select a MIDI output to audition"
            }
            aria-label="Audition drum key"
            onClick={() => void audition()}
          >
            Audition
          </button>
          <button
            type="button"
            disabled={!context || pending || stale}
            title="Remove volume/pan/pitch overrides for this key"
            onClick={() => void resetKeys([note])}
          >
            Reset drum
          </button>
          <button
            type="button"
            disabled={!context || pending || stale}
            title="Remove volume/pan/pitch overrides for every key"
            onClick={() => void resetKeys([])}
          >
            Reset all drums
          </button>
          <button
            className="primary"
            disabled={!context || !changes.length || !valid || pending || stale}
            onClick={() => void apply()}
          >
            Apply Drum Setup
          </button>
        </footer>
      </section>
    </div>
  );
}
