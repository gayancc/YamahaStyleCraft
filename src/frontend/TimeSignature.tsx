import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, WheelEvent } from "react";

const BEATS = Array.from({ length: 16 }, (_, i) => i + 1);
const UNITS: readonly { value: number; note: string }[] = [
  { value: 1, note: "1" },
  { value: 2, note: "1/2" },
  { value: 4, note: "1/4" },
  { value: 8, note: "1/8" },
  { value: 16, note: "1/16" },
];
const PRESETS: readonly (readonly [number, number])[] = [
  [2, 4],
  [3, 4],
  [4, 4],
  [5, 4],
  [6, 8],
  [7, 8],
  [9, 8],
  [12, 8],
];

/** Compact time-signature control for the top bar. A stacked notation chip (beats over beat unit)
 * that changes by wheel or arrow keys, and opens a key panel on click: 16 beat keys, five beat-unit
 * keys and one-tap presets. Every change applies at once as one undoable edit. Replaces the
 * numerator/denominator boxes; FL Studio keeps this under Settings > Project. */
export function TimeSignature({
  numerator,
  denominator,
  disabled,
  onApply,
}: {
  numerator: number;
  denominator: number;
  disabled: boolean;
  onApply: (numerator: number, denominator: number) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", away, true);
    return () => document.removeEventListener("pointerdown", away, true);
  }, [open]);

  const stepBeats = (d: number) => {
    const next = Math.min(16, Math.max(1, numerator + d));
    if (next !== numerator) onApply(next, denominator);
  };
  const stepUnit = (d: number) => {
    const i = UNITS.findIndex((u) => u.value === denominator) + d;
    const next = UNITS[Math.min(UNITS.length - 1, Math.max(0, i))].value;
    if (next !== denominator) onApply(numerator, next);
  };
  const onWheel = (e: WheelEvent<HTMLButtonElement>) => {
    if (disabled) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const dir = e.deltaY < 0 ? 1 : -1;
    if (e.clientY < rect.top + rect.height / 2) stepBeats(dir);
    else stepUnit(dir);
  };
  const onKey = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key === "Escape" && open) {
      e.stopPropagation();
      setOpen(false);
      return;
    }
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    const dir = e.key === "ArrowUp" ? 1 : -1;
    if (e.shiftKey) stepUnit(dir);
    else stepBeats(dir);
  };

  return (
    <div className="time-sig" ref={root} onKeyDown={onKey}>
      <button
        type="button"
        className="time-sig-chip"
        aria-label={`Time signature ${numerator}/${denominator}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Time signature: click to edit. Wheel on the top or bottom number changes beats or beat unit. Up/Down changes beats, Shift+Up/Down the unit."
        data-hint="Time signature"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        onWheel={onWheel}
      >
        <span className="ts-top">{numerator}</span>
        <span className="ts-bar" aria-hidden="true" />
        <span className="ts-bottom">{denominator}</span>
      </button>
      {open && (
        <div
          className="time-sig-panel"
          role="dialog"
          aria-label="Time signature"
        >
          <div className="ts-heading">Beats per bar</div>
          <div className="ts-keys ts-beats">
            {BEATS.map((n) => (
              <button
                key={n}
                type="button"
                className="ts-key"
                aria-pressed={n === numerator}
                aria-label={`${n} beats`}
                onClick={() => onApply(n, denominator)}
              >
                {n}
              </button>
            ))}
          </div>
          <div className="ts-heading">Beat unit</div>
          <div className="ts-keys ts-units">
            {UNITS.map((u) => (
              <button
                key={u.value}
                type="button"
                className="ts-key"
                aria-pressed={u.value === denominator}
                aria-label={`Beat unit ${u.value}`}
                title={`Beat = ${u.note} note`}
                onClick={() => onApply(numerator, u.value)}
              >
                {u.note}
              </button>
            ))}
          </div>
          <div className="ts-heading">Presets</div>
          <div className="ts-keys ts-presets">
            {PRESETS.map(([n, d]) => (
              <button
                key={`${n}/${d}`}
                type="button"
                className="ts-key"
                aria-pressed={n === numerator && d === denominator}
                aria-label={`Preset ${n}/${d}`}
                onClick={() => onApply(n, d)}
              >
                {n}/{d}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
