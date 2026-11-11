import { useEffect, useId, useRef } from "react";
import { useHint } from "./hint";
import {
  angleOf,
  arcPath,
  clamp01,
  dragNorm,
  polar,
  wheelNorm,
} from "./knobMath";

export interface KnobProps {
  /** Parameter name: shown under the knob and in the hint panel. */
  label: string;
  value: number;
  min?: number;
  max?: number;
  onChange: (value: number) => void;
  /** Value used by double-click and Home. Defaults to `min`, or the midpoint when bipolar. */
  defaultValue?: number;
  /** Arc is lit from the 12 o'clock origin instead of from the minimum. */
  bipolar?: boolean;
  /** Arc / dot colour (FL native plugins use orange, green, teal, blue). */
  color?: string;
  /** Outer diameter in px. FL native knobs measure ~38; 20, 30, 36 and ~64 also occur. */
  size?: number;
  /** Text for the hint panel while hovered or dragged. */
  format?: (value: number) => string;
  disabled?: boolean;
  /** Hide the caption (the knob still reports to the hint panel). */
  hideLabel?: boolean;
}

/** FL native knob, measured on Fruity Peak Controller / Convolver:
 * a full dark track ring carrying the coloured value arc, a flat cap that is lighter at the top rim and
 * darker towards the bottom with a dark outline, a small dot for the position, and a notch above the ring at
 * the origin of a bipolar parameter. No readout box: the value is in the hint panel. */
export function Knob({
  label,
  value,
  min = 0,
  max = 1,
  onChange,
  defaultValue,
  bipolar = false,
  color = "var(--fl-orange)",
  size = 38,
  format,
  disabled = false,
  hideLabel = false,
}: KnobProps) {
  const id = useId();
  const owner = useRef({}).current;
  const { show, clear } = useHint();
  const rootRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ y: number; norm: number; fine: boolean } | null>(null);

  const span = max - min;
  const toNorm = (v: number) => (span === 0 ? 0 : clamp01((v - min) / span));
  const fromNorm = (n: number) => min + n * span;
  const norm = toNorm(value);
  const text = (v: number) => (format ? format(v) : String(Math.round(v * 1000) / 1000));
  const reset = defaultValue ?? (bipolar ? (min + max) / 2 : min);

  const emit = (n: number) => {
    const next = fromNorm(n);
    if (next !== value) onChange(next);
    show(owner, label, text(next));
  };

  // React's onWheel is passive; the knob must stop the page scrolling under it.
  const wheelHandler = useRef<(e: WheelEvent) => void>(() => {});
  wheelHandler.current = (e) => {
    if (disabled) return;
    e.preventDefault();
    emit(wheelNorm(norm, e.deltaY, e.ctrlKey || e.shiftKey));
  };
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const fn = (e: WheelEvent) => wheelHandler.current(e);
    el.addEventListener("wheel", fn, { passive: false });
    return () => el.removeEventListener("wheel", fn);
  }, []);
  useEffect(() => () => clear(owner), [clear, owner]);

  // ---- geometry (S = outer diameter) --------------------------------------------------------
  const S = size;
  const c = S / 2;
  const outer = c - 1;
  const ring = Math.max(2, S * 0.066); // FL arc is ~2.5 px on a 38 px knob
  const rArc = outer - ring / 2;
  const rCap = outer - ring - Math.max(1, S * 0.03);
  const cur = angleOf(norm);
  const origin = bipolar ? 0 : angleOf(0);
  const dot = polar(c, c, rCap * 0.74, cur);
  const gradId = `knob-cap-${id}`;

  return (
    <div
      ref={rootRef}
      className={`fl-knob${disabled ? " is-disabled" : ""}`}
      style={{ width: Math.max(S, 44), ["--knob-color" as string]: color }}
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={text(value)}
      aria-disabled={disabled}
      onPointerEnter={() => !disabled && show(owner, label, text(value))}
      onPointerLeave={() => !drag.current && clear(owner)}
      onPointerDown={(e) => {
        if (disabled || e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { y: e.clientY, norm, fine: e.ctrlKey || e.shiftKey };
        e.currentTarget.focus();
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const fine = e.ctrlKey || e.shiftKey;
        // Changing the modifier mid-gesture re-anchors, so the knob never jumps.
        if (fine !== d.fine) drag.current = { y: e.clientY, norm: clamp01(norm), fine };
        const base = drag.current!;
        emit(dragNorm(base.norm, base.y, e.clientY, fine));
      }}
      onPointerUp={(e) => {
        drag.current = null;
        if (!e.currentTarget.contains(document.elementFromPoint(e.clientX, e.clientY))) clear(owner);
      }}
      onDoubleClick={() => !disabled && emit(toNorm(reset))}
      onKeyDown={(e) => {
        if (disabled) return;
        const step = (e.shiftKey ? 1 : 4) / 128;
        const keys: Record<string, number> = {
          ArrowUp: step, ArrowRight: step, PageUp: step * 4,
          ArrowDown: -step, ArrowLeft: -step, PageDown: -step * 4,
        };
        if (e.key in keys) {
          e.preventDefault();
          emit(clamp01(norm + keys[e.key]));
        } else if (e.key === "Home") {
          e.preventDefault();
          emit(toNorm(reset));
        }
      }}
    >
      <svg width={S} height={S} viewBox={`0 0 ${S} ${S}`} overflow="visible" aria-hidden="true">
        <defs>
          <linearGradient id={gradId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="var(--fl-knob-cap-top)" />
            <stop offset="0.18" stopColor="var(--fl-knob-cap)" />
            <stop offset="1" stopColor="var(--fl-knob-cap-bottom)" />
          </linearGradient>
        </defs>
        {/* track: full circle with a darker outer edge */}
        <circle cx={c} cy={c} r={rArc} fill="none" stroke="var(--fl-knob-outline)" strokeWidth={ring + 2} />
        <circle cx={c} cy={c} r={rArc} fill="none" stroke="var(--fl-knob-track)" strokeWidth={ring} />
        {/* value arc on the track */}
        <path d={arcPath(c, c, rArc, origin, cur)} fill="none" stroke="var(--knob-color)" strokeWidth={ring} />
        {bipolar && <rect x={c - 1} y={c - outer - 3} width={2} height={3} fill="var(--knob-color)" />}
        {/* cap: outline, gradient, lighter top rim */}
        <circle cx={c} cy={c} r={rCap + 1} fill="var(--fl-knob-outline)" />
        <circle cx={c} cy={c} r={rCap} fill={`url(#${gradId})`} />
        <path d={arcPath(c, c, rCap - 0.5, -70, 70)} fill="none" stroke="var(--fl-knob-rim)" strokeWidth={1} />
        {/* position dot */}
        <circle cx={dot.x} cy={dot.y} r={Math.max(1.1, S * 0.034)} fill="var(--knob-color)" />
      </svg>
      {!hideLabel && <div className="fl-knob__label">{label}</div>}
    </div>
  );
}
