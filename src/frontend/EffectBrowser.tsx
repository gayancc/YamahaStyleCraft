import { trapModalFocus } from "./modal";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useStudio } from "./store";
import type {
  EffectSummary,
  EffectType,
  EffectSearch,
  View,
} from "./contracts";

type HeaderValues = {
  msb: number | null;
  lsb: number | null;
  returnLevel: number | null;
  lateChanges: boolean;
};
export type StyleEffectHeader = {
  boundary: number;
  reverb: HeaderValues;
  chorus: HeaderValues;
  editProblem: string | null;
};

export function EffectBrowser({
  output,
  onClose,
}: {
  output: { index: number; name: string } | null;
  onClose: () => void;
}) {
  const { view } = useStudio();
  const [original] = useState(view);
  const [header, setHeader] = useState<StyleEffectHeader | null>(null);
  const [headerError, setHeaderError] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<EffectSummary | null>(null);
  const [block, setBlock] = useState<"reverb" | "chorus" | "variation" | "">(
    "reverb",
  );
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<EffectSearch | null>(null);
  const [selected, setSelected] = useState<EffectType | null>(null);
  const [returnLevel, setReturnLevel] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const dialog = useRef<HTMLElement>(null);
  const working = useRef(false);
  const stale = view !== original;
  const selectedValid = selected && (!block || selected.block === block);
  const target = selectedValid ? selected.block : block;
  const fileBlock = target === "reverb" || target === "chorus" ? target : null;
  const parsedReturn = returnLevel.trim() === "" ? null : Number(returnLevel);
  const returnValid =
    parsedReturn === null ||
    (Number.isInteger(parsedReturn) &&
      parsedReturn >= 0 &&
      parsedReturn <= 127);
  const canSave =
    !!original &&
    !!header &&
    !header.editProblem &&
    !stale &&
    !pending &&
    !!fileBlock &&
    returnValid &&
    (!!selectedValid || parsedReturn !== null);

  const canApply =
    !!output &&
    !!selectedValid &&
    (selected.block === "reverb" || selected.block === "chorus") &&
    !pending;

  useEffect(() => {
    let live = true;
    if (original)
      void invoke<StyleEffectHeader>("inspect_style_effects", {
        revision: original.revision,
      })
        .then((h) => {
          if (live) setHeader(h);
        })
        .catch((e) => {
          if (live) setHeaderError(String(e));
        });
    return () => {
      live = false;
    };
  }, [original]);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    let live = true;
    void invoke<EffectSummary | null>("effect_catalog_status")
      .then((value) => {
        if (live) setCatalog(value);
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
    setSelected(null);
    setResult(null);
    if (!catalog) {
      setResult(null);
      return;
    }
    const timer = setTimeout(() => {
      void invoke<EffectSearch>("search_effect_catalog", {
        query,
        block: block || null,
      })
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
  }, [catalog, query, block]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !pending) {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose, pending]);

  const apply = async () => {
    if (
      !canApply ||
      !selected ||
      !output ||
      working.current ||
      useStudio.getState().busy
    )
      return;
    let level: number | null = null;
    if (returnLevel.trim() !== "") {
      const parsed = Number(returnLevel);
      if (!Number.isInteger(parsed) || parsed < 0 || parsed > 127) {
        setError("Return level must be an integer 0–127, or empty to skip.");
        return;
      }
      level = parsed;
    }
    working.current = true;
    useStudio.setState({ busy: true });
    setPending(true);
    setError(null);
    setStatus(null);
    try {
      const applied = await invoke<{ packetCount: number; block: string }>(
        "apply_genos_system_effect",
        {
          index: output.index,
          name: output.name,
          block: selected.block,
          msb: selected.msb,
          lsb: selected.lsb,
          returnLevel: level,
        },
      );
      setStatus(
        `Sent ${applied.packetCount} SysEx packet(s) for ${applied.block} → ${output.name}`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      useStudio.setState({ busy: false });
      setPending(false);
    }
  };

  const importCatalog = async () => {
    if (working.current || useStudio.getState().busy) return;
    working.current = true;
    useStudio.setState({ busy: true });
    setPending(true);
    setError(null);
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Genos Data List", extensions: ["pdf"] }],
      });
      if (typeof path === "string") {
        const next = await invoke<EffectSummary>("import_effect_catalog", {
          path,
        });
        setCatalog(next);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      useStudio.setState({ busy: false });
      setPending(false);
    }
  };

  const saveInStyle = async () => {
    if (
      !canSave ||
      !original ||
      !fileBlock ||
      working.current ||
      useStudio.getState().busy
    )
      return;
    if (useStudio.getState().view !== original) {
      setHeaderError("Document changed; reopen Effect types.");
      return;
    }
    working.current = true;
    useStudio.setState({ busy: true });
    setPending(true);
    setError(null);
    try {
      const next = await invoke<View>("apply_style_effects", {
        revision: original.revision,
        changes: [
          {
            block: fileBlock,
            effect: selectedValid
              ? { msb: selected.msb, lsb: selected.lsb }
              : null,
            returnLevel: parsedReturn,
          },
        ],
      });
      useStudio.setState({
        view: next,
        message:
          next.revision === original.revision
            ? "Style effect unchanged"
            : "Style header effect applied · one undo step",
      });
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      useStudio.setState({ busy: false });
      setPending(false);
    }
  };

  return (
    <div
      className="modal-backdrop"
      onClick={() => {
        if (!pending) onClose();
      }}
    >
      <section
        className="modal voice-browser effect-browser"
        role="dialog"
        aria-modal="true"
        aria-labelledby="effect-browser-title"
        ref={dialog}
        tabIndex={-1}
        onKeyDown={trapModalFocus}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-title">
          <h2 id="effect-browser-title">Genos effect types</h2>
          <button type="button" onClick={onClose} disabled={pending}>
            Close
          </button>
        </div>
        <div className="voice-browser-body">
          <p className="muted">
            Effect Type List from the imported Genos v2.00 Data List (528
            types). Apply to Style saves the initial reverb/chorus type and
            optional return in the file, as one undo step, without sending MIDI.
            Apply to Genos changes only the connected instrument. Variation /
            insertion types can be browsed here.
          </p>
          {!catalog && (
            <p className="muted">
              Import the supported Genos v2.00 English Data List PDF (11/2019)
              to choose effect types. Return alone can be saved without a
              catalog.
            </p>
          )}
          <button
            type="button"
            disabled={pending}
            onClick={() => void importCatalog()}
          >
            Import effect Data List PDF
          </button>
          {catalog && (
            <p className="muted">
              {catalog.count} types · reverb {catalog.reverb} · chorus{" "}
              {catalog.chorus} · variation {catalog.variation}
            </p>
          )}
          <div className="drum-preview-row">
            <label className="field">
              <span>Block</span>
              <select
                aria-label="Effect block"
                value={block}
                disabled={pending}
                onChange={(e) => {
                  setBlock(
                    e.target.value as "reverb" | "chorus" | "variation" | "",
                  );
                  setSelected(null);
                  setReturnLevel("");
                  setStatus(null);
                }}
              >
                <option value="reverb">Reverb</option>
                <option value="chorus">Chorus</option>
                <option value="variation">Variation / Insertion</option>
                <option value="">All blocks</option>
              </select>
            </label>
            <label className="field">
              <span>Search</span>
              <input
                aria-label="Search effect types"
                value={query}
                disabled={!catalog || pending}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setSelected(null);
                }}
              />
            </label>
            <label className="field">
              <span>Return (optional)</span>
              <input
                aria-label="System effect return level"
                inputMode="numeric"
                placeholder="0–127"
                value={returnLevel}
                disabled={pending}
                onChange={(e) => setReturnLevel(e.target.value)}
              />
            </label>
          </div>
          {header && fileBlock && (
            <div className="voice-confirm" aria-label="Initial Style effect">
              <strong>Initial {fileBlock}</strong>
              <p>
                Type{" "}
                {header[fileBlock].msb === null ||
                header[fileBlock].lsb === null
                  ? "unspecified"
                  : `MSB ${header[fileBlock].msb} · LSB ${header[fileBlock].lsb}`}{" "}
                · Return {header[fileBlock].returnLevel ?? "unspecified"}
              </p>
              <p className="muted">
                Applies before the first section. Original SysEx and other
                settings stay in the file. Blank Return keeps its existing
                state. Enable Preview initial Genos effects in Devices to send
                known initial type/Return values before Play. Detailed
                parameters and later effect messages are not applied.
              </p>
              {header[fileBlock].lateChanges && (
                <p className="muted">
                  Later effect messages exist and may replace this initial
                  setting.
                </p>
              )}
            </div>
          )}
          {!original && (
            <p className="muted">Open a Style to author its initial effects.</p>
          )}
          {headerError && (
            <p className="error" role="alert">
              Style file: {headerError}
            </p>
          )}
          {header?.editProblem && (
            <p className="error" role="alert">
              Style file: {header.editProblem}
            </p>
          )}
          {stale && (
            <p className="error">
              Document changed. Close and reopen Effect types before applying to
              Style.
            </p>
          )}
          {!returnValid && (
            <p className="error">
              Return level must be a whole number 0–127, or blank to keep it.
            </p>
          )}
          <div
            className="voice-results"
            role="listbox"
            aria-label="Effect types"
          >
            {result?.effects.map((e) => (
              <button
                key={`${e.block}-${e.msb}-${e.lsb}-${e.name}`}
                type="button"
                role="option"
                disabled={pending}
                aria-selected={
                  selected?.name === e.name &&
                  selected.msb === e.msb &&
                  selected.lsb === e.lsb
                }
                className={
                  selected?.name === e.name &&
                  selected.msb === e.msb &&
                  selected.lsb === e.lsb
                    ? "selected"
                    : undefined
                }
                onClick={() => {
                  setSelected(e);
                  setStatus(null);
                }}
              >
                <strong>{e.name}</strong>
                <span>
                  {e.block} · {e.category} · MSB {e.msb} LSB {e.lsb}
                </span>
              </button>
            ))}
          </div>
          {result && (
            <p className="muted">
              Showing {result.effects.length} of {result.total}
            </p>
          )}
          {selected && (
            <div className="voice-confirm" aria-label="Selected effect type">
              <p>
                <strong>{selected.name}</strong>
              </p>
              <p className="muted">
                {selected.description || "No description extracted."}
              </p>
              <p>
                Block {selected.block} · MSB {selected.msb} · LSB {selected.lsb}{" "}
                · page {selected.page}
              </p>
              <p className="muted">
                Parameter group: {selected.parameterGroup || "—"}
                {selected.block === "variation"
                  ? " (variation / insertion authoring is unavailable)."
                  : " · Choose Apply to Style for file editing or Apply to Genos for the connected instrument."}
              </p>
            </div>
          )}
          {!output && (
            <p className="muted">
              Select a MIDI output in Devices to apply SysEx.
            </p>
          )}
          {status && <p className="muted">{status}</p>}
          {error && <p className="error">{error}</p>}
        </div>
        <div className="modal-actions">
          <button
            type="button"
            className="primary"
            disabled={!canSave}
            onClick={() => void saveInStyle()}
          >
            Apply to Style
          </button>
          <button
            type="button"
            className="secondary"
            disabled={!canApply}
            onClick={() => void apply()}
          >
            Apply to Genos
          </button>
          <button type="button" onClick={onClose} disabled={pending}>
            Close
          </button>
        </div>
      </section>
    </div>
  );
}
