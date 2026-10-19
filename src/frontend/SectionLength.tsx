import { useEffect, useMemo, useRef, useState } from "react";
import type { Section, View } from "./contracts";
import { trapModalFocus } from "./modal";
import { useStudio } from "./store";

function isFillBreak(label: string): boolean {
  const n = label.trim().toUpperCase();
  return (
    [
      "FILL IN AA",
      "FILL IN BB",
      "FILL IN CC",
      "FILL IN DD",
      "FILL IN BA",
      "BREAK",
    ].includes(n) || n.startsWith("FILL ")
  );
}

function barsOf(view: View, section: Section): number {
  const bar = ((view.ppq * 4) / view.denominator) * view.numerator;
  return (section.end - section.start) / bar;
}

export function SectionLength({
  view,
  section,
  onClose,
}: {
  view: View;
  section: Section;
  onClose: () => void;
}) {
  const { edit, busy } = useStudio();
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const currentBars = Math.round(barsOf(view, section));
  const fillBreak = isFillBreak(section.label);
  const [mode, setMode] = useState<"fillBreak" | "shorten">(
    fillBreak ? "fillBreak" : "shorten",
  );
  const [bars, setBars] = useState(
    fillBreak ? Math.min(8, Math.max(1, currentBars)) : Math.max(1, currentBars - 1),
  );
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    return () => prior?.focus();
  }, []);

  const maxBars = mode === "fillBreak" ? 8 : Math.max(1, currentBars - 1);
  const minBars = 1;
  const preview = useMemo(() => {
    if (mode === "shorten") {
      return `Shorten ${section.label}: ${currentBars} → ${bars} bars. Notes past the new end are removed; crossing notes are clipped. CASM and Voices stay.`;
    }
    return `Set ${section.label} length to ${bars} bars (1–8). Shortening rejects if MIDI would be lost — use Shorten Section instead.`;
  }, [mode, section.label, currentBars, bars]);

  const dismiss = () => {
    if (!busy) close.current();
  };

  const apply = async () => {
    if (busy) return;
    if (mode === "shorten" && !confirm) {
      setError("Confirm deleting data after the new section end.");
      return;
    }
    setError(null);
    await edit({
      type: "sectionLength",
      section: section.id,
      op:
        mode === "fillBreak"
          ? { type: "fillBreak", bars }
          : { type: "shorten", bars },
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
        aria-labelledby="section-length-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={trapModalFocus}
      >
        <div className="modal-title">
          <h2 id="section-length-title">
            {mode === "fillBreak" ? "Fill / Break length" : "Shorten Section"}
          </h2>
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
          {section.label} · current {currentBars} bars
        </p>
        <fieldset className="field" disabled={busy}>
          {fillBreak && (
            <label>
              <span>Tool</span>
              <select
                aria-label="Section length tool"
                value={mode}
                onChange={(e) => {
                  const next = e.target.value as "fillBreak" | "shorten";
                  setMode(next);
                  setConfirm(false);
                  setBars(
                    next === "fillBreak"
                      ? Math.min(8, Math.max(1, currentBars))
                      : Math.max(1, currentBars - 1),
                  );
                }}
              >
                <option value="fillBreak">Fill / Break length</option>
                <option value="shorten">Shorten Section…</option>
              </select>
            </label>
          )}
          <label>
            <span>{mode === "shorten" ? "New length (bars)" : "Length in bars"}</span>
            <input
              aria-label={mode === "shorten" ? "New length in bars" : "Length in bars"}
              type="number"
              min={minBars}
              max={maxBars}
              step={1}
              value={bars}
              onChange={(e) => {
                setConfirm(false);
                setBars(
                  Math.min(
                    maxBars,
                    Math.max(minBars, Math.round(Number(e.target.value) || minBars)),
                  ),
                );
              }}
            />
          </label>
        </fieldset>
        <p className="source-zone">{preview}</p>
        {mode === "shorten" && (
          <label className="source-check">
            <input
              type="checkbox"
              checked={confirm}
              disabled={busy}
              onChange={(e) => setConfirm(e.target.checked)}
            />{" "}
            I confirm deleting data after the new section end
          </label>
        )}
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
            disabled={
              busy ||
              (mode === "shorten" && (!confirm || bars >= currentBars)) ||
              (mode === "fillBreak" && bars === currentBars)
            }
            onClick={() => void apply()}
          >
            {busy
              ? "Applying…"
              : mode === "shorten"
                ? "Shorten Section"
                : "Apply Fill / Break length"}
          </button>
        </div>
      </section>
    </div>
  );
}
