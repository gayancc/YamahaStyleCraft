import { trapModalFocus } from "./modal";
import { NativeDrumSetup } from "./NativeDrumSetup";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStudio } from "./store";
import type {
  CatalogSummary,
  CatalogVoice,
  DrumChannelStatus,
  Section,
  VoiceSearch,
  View,
} from "./contracts";

export function DrumKits({
  section,
  channel,
  output,
  onClose,
}: {
  section: Section;
  channel: number;
  output: { index: number; name: string } | null;
  onClose: () => void;
}) {
  const { view } = useStudio();
  const [native, setNative] = useState(false);
  const [original] = useState(view!);
  const [catalog, setCatalog] = useState<CatalogSummary | null>(null);
  const [owners, setOwners] = useState<DrumChannelStatus[]>([]);
  const [selected, setSelected] = useState<CatalogVoice | null>(null);
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<VoiceSearch | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [pitch, setPitch] = useState(36);
  const [previewChannel, setPreviewChannel] = useState(
    channel === 9 || channel === 10 ? channel : 10,
  );
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  const working = useRef(false);
  close.current = onClose;
  const stale = view !== original;

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    let live = true;
    void invoke<CatalogSummary | null>("voice_catalog_status")
      .then((value) => {
        if (live) setCatalog(value);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    void invoke<DrumChannelStatus[]>("drum_channel_status", {
      section: section.id,
      revision: original.revision,
    })
      .then((value) => {
        if (live) setOwners(value);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
      previous?.focus();
    };
  }, [original.revision, section.id]);

  useEffect(() => {
    let live = true;
    if (!catalog) {
      setResult(null);
      return;
    }
    const timer = setTimeout(() => {
      void invoke<VoiceSearch>("search_drum_kits", { query })
        .then((value) => {
          if (live) setResult(value);
        })
        .catch((e) => {
          if (live) setError(String(e));
        });
    }, 120);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [catalog, query]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !pending && !native) {
        e.preventDefault();
        close.current();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [pending, native]);

  const apply = async () => {
    if (!selected || working.current || stale || useStudio.getState().busy)
      return;
    working.current = true;
    setPending(true);
    useStudio.setState({ busy: true });
    setError(null);
    try {
      const next = await invoke<View>("apply_catalog_voice", {
        section: section.id,
        channel: previewChannel,
        revision: original.revision,
        address: {
          msb: selected.msb,
          lsb: selected.lsb,
          program: selected.program,
        },
      });
      useStudio.setState({ view: next, message: `Applied ${selected.name}` });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      useStudio.setState({ busy: false });
      setPending(false);
    }
  };

  const preview = async () => {
    if (!output || working.current || stale || useStudio.getState().busy)
      return;
    if (!Number.isInteger(pitch) || pitch < 13 || pitch > 91) {
      setError("Enter a drum note from 13 to 91.");
      return;
    }
    working.current = true;
    setPending(true);
    useStudio.setState({ busy: true });
    setError(null);
    try {
      await invoke("preview_drum_note", {
        index: output.index,
        name: output.name,
        channel: previewChannel,
        pitch,
        velocity: 90,
        holdMs: 200,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      useStudio.setState({ busy: false });
      setPending(false);
    }
  };

  if (native)
    return (
      <NativeDrumSetup
        channel={previewChannel}
        output={output}
        onClose={onClose}
      />
    );
  return (
    <div
      className="modal-backdrop"
      onClick={() => {
        if (!pending) onClose();
      }}
    >
      <section
        className="modal voice-browser drum-kits"
        role="dialog"
        aria-modal="true"
        aria-labelledby="drum-kits-title"
        ref={dialog}
        tabIndex={-1}
        onKeyDown={trapModalFocus}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-title">
          <h2 id="drum-kits-title">Drum kits</h2>
          <button type="button" disabled={pending} onClick={onClose}>
            Close
          </button>
        </div>
        <div className="voice-browser-body">
          <button
            type="button"
            disabled={pending || stale}
            onClick={() => setNative(true)}
          >
            Genos per-key setup…
          </button>
          <p className="muted">
            Genos factory drum-kit programs, included with the application.
            Apply sets the kit at the section start on channel 9 or 10.
            Following sections inherit it until another Program Change. Audition
            plays the instrument's current kit; selecting a row does not send
            that kit. A kit Program Change can reset per-key settings.
          </p>
          {!catalog && <p className="muted">Loading Genos drum kits…</p>}
          {owners.length > 0 && (
            <ul className="drum-owners" aria-label="Rhythm channel kits">
              {owners.map((o) => (
                <li key={o.channel}>
                  Ch {o.channel}:{" "}
                  {o.voice?.name ?? "No named kit at section start"}
                  {o.lateProgram ? " · late Program Change in section" : ""}
                </li>
              ))}
            </ul>
          )}
          <label className="field">
            <span>Search DrumKit catalog</span>
            <input
              value={query}
              disabled={!catalog || pending}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Search drum kits"
            />
          </label>
          <div className="voice-results" role="listbox" aria-label="Drum kits">
            {result?.voices.map((v) => (
              <button
                key={`${v.msb}-${v.lsb}-${v.program}`}
                type="button"
                role="option"
                aria-selected={selected?.name === v.name}
                className={selected?.name === v.name ? "selected" : undefined}
                disabled={pending || stale}
                onClick={() => setSelected(v)}
              >
                <strong>{v.name}</strong>
                <span>
                  {v.voiceType} · MSB {v.msb} LSB {v.lsb} PC {v.program + 1}
                </span>
              </button>
            ))}
          </div>
          {result && (
            <p className="muted">
              Showing {result.voices.length} of {result.total} DrumKit rows
            </p>
          )}
          <div className="drum-preview-row">
            <label className="field">
              <span>Target / audition channel</span>
              <select
                aria-label="Drum preview channel"
                value={previewChannel}
                disabled={pending}
                onChange={(e) => setPreviewChannel(Number(e.target.value))}
              >
                <option value={9}>9</option>
                <option value={10}>10</option>
              </select>
            </label>
            <label className="field">
              <span>Preview note (13–91)</span>
              <input
                type="number"
                min={13}
                max={91}
                aria-label="Drum preview note"
                value={pitch}
                disabled={pending}
                onChange={(e) => setPitch(Number(e.target.value))}
              />
            </label>
            <button
              type="button"
              disabled={!output || pending || stale}
              onClick={() => void preview()}
            >
              Audition note
            </button>
          </div>
          {error && <p className="error">{error}</p>}
          {stale && (
            <p className="muted">
              Document changed — close and reopen drum kits.
            </p>
          )}
        </div>
        <div className="modal-actions">
          <button type="button" disabled={pending} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="primary"
            disabled={!selected || pending || stale || !catalog}
            onClick={() => void apply()}
          >
            Apply kit to Ch {previewChannel}
          </button>
        </div>
      </section>
    </div>
  );
}
