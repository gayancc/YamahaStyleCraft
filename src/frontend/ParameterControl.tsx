import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";

/** A musical parameter: local feedback during adjustment, one history edit on release. */
export function ParameterControl({
  label,
  ariaLabel = label,
  value,
  min = 0,
  max = 127,
  step = 1,
  resetValue = min,
  bipolar = false,
  variant = "knob",
  disabled = false,
  format = String,
  onCommit,
}: {
  label: string;
  ariaLabel?: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  resetValue?: number;
  bipolar?: boolean;
  variant?: "knob" | "fader";
  disabled?: boolean;
  format?: (n: number) => string;
  onCommit: (n: number) => void;
}) {
  const [draft, setDraft] = useState(value),
    [entry, setEntry] = useState<string | null>(null);
  const current = useRef(value),
    committed = useRef(value);
  const drag = useRef<{
    id: number;
    y: number;
    amount: number;
    original: number;
  } | null>(null);
  const wasDisabled = useRef(disabled);
  const cancelEntry = useRef(false);
  const control = useRef<HTMLDivElement>(null);
  const normalize = (n: number) =>
    Math.min(max, Math.max(min, min + Math.round((n - min) / step) * step));
  const update = (n: number) => {
    current.current = normalize(n);
    setDraft(current.current);
  };
  useEffect(() => {
    if (!drag.current) {
      current.current = value;
      committed.current = value;
      setDraft(value);
    }
  }, [value]);
  // A failed native edit leaves the value unchanged. Restore it when the busy cycle finishes.
  useEffect(() => {
    if (disabled) setEntry(null);
    if (disabled && drag.current) {
      drag.current = null;
      update(value);
    }
    if (wasDisabled.current && !disabled) {
      committed.current = value;
      update(value);
    }
    wasDisabled.current = disabled;
  }, [disabled, value]);
  const commit = () => {
    if (disabled || current.current === committed.current) return;
    committed.current = current.current;
    onCommit(current.current);
  };
  const cancel = () => {
    const original = drag.current?.original ?? value;
    drag.current = null;
    update(original);
  };
  const ratio = (draft - min) / (max - min || 1),
    angle = bipolar
      ? draft <= resetValue
        ? (-135 * (resetValue - draft)) / Math.max(1, resetValue - min)
        : (135 * (draft - resetValue)) / Math.max(1, max - resetValue)
      : -135 + ratio * 270;
  const point = (a: number) => {
    const r = (a * Math.PI) / 180;
    return [24 + 20 * Math.sin(r), 24 - 20 * Math.cos(r)];
  };
  const arc = (from: number, to: number) => {
    const a = point(from),
      b = point(to);
    return `M ${a[0]} ${a[1]} A 20 20 0 ${Math.abs(to - from) > 180 ? 1 : 0} ${to >= from ? 1 : 0} ${b[0]} ${b[1]}`;
  };
  return (
    <div
      className={`parameter parameter-${variant}${bipolar ? " bipolar" : ""}${disabled ? " disabled" : ""}`}
    >
      <span className="parameter-label">{label}</span>
      <div
        ref={control}
        className="parameter-surface"
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={ariaLabel}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={draft}
        aria-valuetext={format(draft)}
        aria-disabled={disabled}
        aria-orientation="vertical"
        title={`${ariaLabel}: ${format(draft)} · Drag up/down · Shift for fine adjustment · Double-click to reset · Click value to type`}
        style={
          {
            "--parameter-ratio": ratio,
            "--parameter-angle": `${angle}deg`,
          } as CSSProperties
        }
        onPointerDown={(e) => {
          if (disabled || drag.current || e.button !== 0) return;
          e.preventDefault();
          e.currentTarget.focus();
          e.currentTarget.setPointerCapture(e.pointerId);
          drag.current = {
            id: e.pointerId,
            y: e.clientY,
            amount: current.current,
            original: current.current,
          };
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d || disabled) return;
          // Integrate each movement so toggling Shift never jumps the value.
          d.amount +=
            (((d.y - e.clientY) * (max - min)) /
              (variant === "fader" ? 170 : 180)) *
            (e.shiftKey ? 0.1 : 1);
          d.amount = Math.min(max, Math.max(min, d.amount));
          d.y = e.clientY;
          update(d.amount);
        }}
        onPointerUp={(e) => {
          if (!drag.current || drag.current.id !== e.pointerId) return;
          drag.current = null;
          commit();
          if (e.currentTarget.hasPointerCapture(e.pointerId))
            e.currentTarget.releasePointerCapture(e.pointerId);
        }}
        onPointerCancel={cancel}
        onLostPointerCapture={() => {
          if (drag.current) cancel();
        }}
        onDoubleClick={() => {
          if (!disabled) {
            update(resetValue);
            commit();
          }
        }}
        onKeyDown={(e) => {
          if (disabled) return;
          const k = e.key;
          if (k === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            cancel();
            return;
          }
          if (
            ![
              "ArrowUp",
              "ArrowDown",
              "ArrowLeft",
              "ArrowRight",
              "PageUp",
              "PageDown",
              "Home",
              "End",
            ].includes(k)
          )
            return;
          e.preventDefault();
          e.stopPropagation();
          const delta = k === "PageUp" || k === "PageDown" ? step * 10 : step;
          update(
            k === "Home"
              ? min
              : k === "End"
                ? max
                : current.current +
                  (["ArrowUp", "ArrowRight", "PageUp"].includes(k)
                    ? delta
                    : -delta),
          );
        }}
        onKeyUp={(e) => {
          if (
            [
              "ArrowUp",
              "ArrowDown",
              "ArrowLeft",
              "ArrowRight",
              "PageUp",
              "PageDown",
              "Home",
              "End",
            ].includes(e.key)
          ) {
            e.stopPropagation();
            commit();
          }
        }}
        onBlur={() => {
          if (!drag.current) commit();
        }}
      >
        {variant === "knob" ? (
          <svg viewBox="0 0 48 48" aria-hidden="true">
            <path className="knob-track" d={arc(-135, 135)} />
            <path className="knob-amount" d={arc(bipolar ? 0 : -135, angle)} />
            <circle className="knob-body" cx="24" cy="24" r="15" />
            <line
              className="knob-pointer"
              x1="24"
              y1="14"
              x2="24"
              y2="19"
              transform={`rotate(${angle} 24 24)`}
            />
            {bipolar && <path className="knob-center" d="M24 1 V5" />}
          </svg>
        ) : (
          <>
            <div className="fader-scale" aria-hidden="true">
              <span>127</span>
              <span>64</span>
              <span>0</span>
            </div>
            <div className="fader-track" aria-hidden="true" />
            <div className="fader-thumb" aria-hidden="true" />
          </>
        )}
      </div>
      {entry !== null ? (
        <input
          className="parameter-entry"
          type="number"
          autoFocus
          aria-label={`${ariaLabel} value`}
          min={min}
          max={max}
          step={step}
          value={entry}
          disabled={disabled}
          onChange={(e) => setEntry(e.target.value)}
          onFocus={(e) => e.currentTarget.select()}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape") {
              cancelEntry.current = true;
              setEntry(null);
              control.current?.focus();
            }
            if (e.key === "Enter") {
              e.preventDefault();
              e.currentTarget.blur();
            }
          }}
          onBlur={() => {
            if (cancelEntry.current) {
              cancelEntry.current = false;
              return;
            }
            const n = entry.trim() ? Number(entry) : NaN;
            if (Number.isFinite(n)) {
              update(n);
              commit();
            }
            setEntry(null);
          }}
        />
      ) : (
        <button
          type="button"
          className="parameter-value"
          disabled={disabled}
          aria-label={`Enter ${ariaLabel} value`}
          title="Click to enter an exact value"
          onClick={() => {
            cancelEntry.current = false;
            setEntry(String(current.current));
          }}
        >
          {format(draft)}
        </button>
      )}
    </div>
  );
}

export const formatPan = (n: number) =>
  n === 64 ? "C" : n < 64 ? `L ${64 - n}` : `R ${n - 64}`;
export const formatBipolar = (n: number) =>
  n === 64 ? "0" : `${n > 64 ? "+" : ""}${n - 64}`;
