import { restoreEditorFocus } from "./modal";
import { useEffect, useRef, useState } from "react";
import type { Section, View } from "./contracts";
import { trapModalFocus } from "./modal";
import { useStudio } from "./store";

type ScopeKind = "source" | "part" | "section";
type FitKind = "keep" | "repeat" | "fit";

export function CopyMaterial({
  view,
  source,
  channel,
  onClose,
}: {
  view: View;
  source: Section;
  channel: number;
  onClose: () => void;
}) {
  const { edit, busy } = useStudio();
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const destinations = view.sections.filter((s) => s.id !== source.id);
  const [destinationId, setDestinationId] = useState(
    destinations[0]?.id ?? source.id,
  );
  const [scope, setScope] = useState<ScopeKind>("source");
  const [fit, setFit] = useState<FitKind>("keep");
  const [error, setError] = useState<string | null>(null);
  const destination = view.sections.find((s) => s.id === destinationId);
  const srcBars = bars(view, source);
  const destBars = destination ? bars(view, destination) : 0;

  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    return () => restoreEditorFocus(prior);
  }, []);

  const dismiss = () => {
    if (!busy) close.current();
  };

  const apply = async () => {
    if (!destination || destination.id === source.id || busy) return;
    setError(null);
    const scopePayload =
      scope === "section"
        ? { type: "section" as const }
        : scope === "part"
          ? { type: "part" as const, channel }
          : { type: "source" as const, channel };
    await edit({
      type: "copyMaterial",
      sourceSection: source.id,
      destinationSection: destination.id,
      scope: scopePayload,
      fit,
    });
    const state = useStudio.getState();
    if (state.error) setError(state.error);
    else close.current();
  };

  return (
    <div
      className="modal-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" && !busy) {
          e.preventDefault();
          close.current();
        }
      }}
    >
      <section
        ref={dialog}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="copy-material-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={trapModalFocus}
      >
        <div className="modal-title">
          <h2 id="copy-material-title">Copy material</h2>
          <button
            type="button"
            aria-label="Close"
            disabled={busy}
            onClick={dismiss}
          >
            ×
          </button>
        </div>
        <p className="source-zone">
          Copy notes and channel controllers from{" "}
          <strong>{source.label}</strong> into another section. CASM routes,
          SysEx, OTS, and project tempo are not copied. Destination material in
          the selected scope is replaced in one undo step.
        </p>
        <fieldset className="field" disabled={busy}>
          <label>
            <span>Destination section</span>
            <select
              aria-label="Destination section"
              value={destinationId}
              disabled={!destinations.length}
              onChange={(e) => setDestinationId(Number(e.target.value))}
            >
              {destinations.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label} · {bars(view, s).toFixed(1)} bars
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Scope</span>
            <select
              aria-label="Copy scope"
              value={scope}
              onChange={(e) => setScope(e.target.value as ScopeKind)}
            >
              <option value="source">Source CH{channel}</option>
              <option value="part">Part (all sources → same destination)</option>
              <option value="section">Entire section</option>
            </select>
          </label>
          <label>
            <span>Length fit</span>
            <select
              aria-label="Copy length fit"
              value={fit}
              onChange={(e) => setFit(e.target.value as FitKind)}
            >
              <option value="keep">Keep timing (reject if too long)</option>
              <option value="repeat">Repeat / tile into destination</option>
              <option value="fit">Fit / stretch to destination</option>
            </select>
          </label>
        </fieldset>
        <p className="source-zone">
          Source {srcBars.toFixed(1)} bars → destination {destBars.toFixed(1)}{" "}
          bars
          {fit === "keep" && srcBars > destBars + 0.01
            ? " · Keep will reject if phrases overrun"
            : ""}
        </p>
        {error && (
          <p role="alert" className="source-error">
            {error}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" disabled={busy} onClick={dismiss}>
            Cancel
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || !destinations.length}
            onClick={() => void apply()}
          >
            {busy ? "Copying…" : "Copy"}
          </button>
        </div>
      </section>
    </div>
  );
}

function bars(view: View, section: Section): number {
  const bar = ((view.ppq * 4) / view.denominator) * view.numerator;
  return (section.end - section.start) / bar;
}
