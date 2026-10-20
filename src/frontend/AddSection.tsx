import { useEffect, useMemo, useRef, useState } from "react";
import type { Section, View } from "./contracts";
import { restoreEditorFocus, trapModalFocus } from "./modal";
import {
  defaultSlotBars,
  layoutSections,
  slotKeyFor,
  type SectionSlot,
  type SlotGroupName,
} from "./sectionSlots";
import { useStudio } from "./store";

type Fit = "keep" | "repeat" | "fit";

function barsOf(view: View, section: Section): number {
  const bar = ((view.ppq * 4) / view.denominator) * view.numerator;
  return Math.max(1, Math.round((section.end - section.start) / bar));
}

/** Adds a Genos panel section the loaded Style does not have: empty, or a copy of an existing
 * section (how StyleMagicYA fills an empty section), with a length in bars within the SFM Style
 * Studio section-setup limits. The engine writes the marker and the `fn:` description and shares
 * the CASM routes of the chosen section. */
export function AddSection({
  view,
  slot,
  group,
  onClose,
}: {
  view: View;
  slot: SectionSlot<Section>;
  group: SlotGroupName;
  onClose: () => void;
}) {
  const { edit, busy } = useStudio();
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  const layout = useMemo(() => layoutSections(view.sections), [view.sections]);
  const present = useMemo(
    () =>
      layout.groups
        .flatMap((g) => g.slots)
        .flatMap((s) => (s.section ? [s.section] : [])),
    [layout],
  );
  const sameFamily = useMemo(
    () =>
      layout.groups
        .find((g) => g.name === group)
        ?.slots.find((s) => s.section)?.section ?? null,
    [layout, group],
  );
  const { bars: defaultBars, max: maxBars } = defaultSlotBars(group);
  const [copy, setCopy] = useState(false);
  const [copyFrom, setCopyFrom] = useState<number>(present[0]?.id ?? 0);
  const [bars, setBars] = useState(defaultBars);
  const [casmFrom, setCasmFrom] = useState<number>(
    sameFamily?.id ?? present[0]?.id ?? 0,
  );
  const [fit, setFit] = useState<Fit | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("input,select,button")?.focus();
    return () => restoreEditorFocus(prior);
  }, []);

  const source = present.find((s) => s.id === copyFrom) ?? null;
  const sourceBars = source ? barsOf(view, source) : 0;
  const autoFit: Fit =
    !copy || bars === sourceBars
      ? "keep"
      : bars > sourceBars && bars % sourceBars === 0
        ? "repeat"
        : "fit";
  const validBars = Number.isInteger(bars) && bars >= 1 && bars <= maxBars;
  const problem = !present.length
    ? "This Style has no section to base the new one on."
    : !validBars
      ? `Length must be 1–${maxBars} bars.`
      : null;

  const chooseCopy = (id: number) => {
    setCopyFrom(id);
    setCasmFrom(id);
    const chosen = present.find((s) => s.id === id);
    if (chosen) setBars(Math.min(maxBars, barsOf(view, chosen)));
    setFit(null);
  };
  const toggleCopy = (on: boolean) => {
    setCopy(on);
    if (on) chooseCopy(copyFrom);
    else {
      setBars(defaultBars);
      setCasmFrom(sameFamily?.id ?? present[0]?.id ?? 0);
    }
  };
  const dismiss = () => {
    if (!busy) close.current();
  };
  const apply = async () => {
    if (busy || problem) return;
    setError(null);
    await edit({
      type: "addSection",
      label: slot.yamahaLabel,
      bars,
      casmFrom,
      copyFrom: copy ? copyFrom : null,
      fit: copy ? (fit ?? autoFit) : null,
    });
    const state = useStudio.getState();
    if (state.error) {
      setError(state.error);
      return;
    }
    const added = state.view?.sections.find(
      (s) => slotKeyFor(s.label) === slot.key,
    );
    if (added) state.section(added.id);
    close.current();
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
        className="modal add-section-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-section-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={trapModalFocus}
      >
        <div className="modal-title">
          <h2 id="add-section-title">Add {slot.label}</h2>
          <button
            type="button"
            aria-label="Close"
            disabled={busy}
            onClick={dismiss}
          >
            ×
          </button>
        </div>
        <p>
          This Style has no {slot.label} ({slot.yamahaLabel}). Add it empty, or
          start from a copy of another section.
        </p>
        <fieldset className="fl-plain" disabled={busy}>
          <fieldset className="fl-section fl-rows">
            <legend>Length</legend>
            <label className="field">
              <span>Bars (1–{maxBars})</span>
              <input
                aria-label="Length in bars"
                type="number"
                min={1}
                max={maxBars}
                step={1}
                value={Number.isNaN(bars) ? "" : bars}
                onChange={(e) =>
                  setBars(
                    e.target.value === "" ? Number.NaN : Number(e.target.value),
                  )
                }
              />
            </label>
          </fieldset>
          <fieldset className="fl-section fl-rows">
            <legend>Start from</legend>
            <label className="check">
              <input
                type="radio"
                name="add-section-start"
                checked={!copy}
                onChange={() => toggleCopy(false)}
              />
              <span>Empty section</span>
            </label>
            <label className="check">
              <input
                type="radio"
                name="add-section-start"
                checked={copy}
                disabled={!present.length}
                onChange={() => toggleCopy(true)}
              />
              <span>Copy of an existing section</span>
            </label>
            {copy && (
              <>
                <label className="field">
                  <span>Copy from</span>
                  <select
                    aria-label="Copy from section"
                    value={copyFrom}
                    onChange={(e) => chooseCopy(Number(e.target.value))}
                  >
                    {present.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.label} · {barsOf(view, s)} bars
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  <span>Fit copied timing</span>
                  <select
                    aria-label="Fit copied timing"
                    value={fit ?? autoFit}
                    onChange={(e) => setFit(e.target.value as Fit)}
                  >
                    <option value="keep">Keep timing</option>
                    <option value="repeat">Repeat to length</option>
                    <option value="fit">Fit to length</option>
                  </select>
                </label>
              </>
            )}
          </fieldset>
          <fieldset className="fl-section fl-rows">
            <legend>CASM routes</legend>
            <label className="field">
              <span>Share routes with</span>
              <select
                aria-label="Share CASM routes with"
                value={casmFrom}
                onChange={(e) => setCasmFrom(Number(e.target.value))}
              >
                {present.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.label}
                  </option>
                ))}
              </select>
            </label>
            <p className="fl-note">
              Adds the section marker and description a Yamaha Style needs.
              Hardware acceptance of a Style with an added section has not been
              verified.
            </p>
          </fieldset>
        </fieldset>
        {(problem || error) && (
          <p role="alert" className="source-error">
            {error ?? problem}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" disabled={busy} onClick={dismiss}>
            Cancel
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || Boolean(problem)}
            onClick={() => void apply()}
          >
            {busy ? "Adding…" : "Add section"}
          </button>
        </div>
      </section>
    </div>
  );
}
