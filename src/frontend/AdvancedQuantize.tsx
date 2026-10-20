import { useEffect, useRef, useState } from "react";
import type {
  Note,
  QuantizeGrid,
  QuantizeOptions,
  Section,
  View,
} from "./contracts";
import { ParameterControl } from "./ParameterControl";
import { QuantizePreview } from "./QuantizePreview";
import { useStudio } from "./store";

export const quantizeGrids: [QuantizeGrid, string][] = [
  ["480", "1/4"],
  ["320", "1/4 Triplet"],
  ["240", "1/8"],
  ["160", "1/8 Triplet"],
  ["120", "1/16"],
  ["80", "1/16 Triplet"],
  ["mixed8", "1/8 + 1/8 Triplet"],
  ["mixed16", "1/16 + 1/16 Triplet"],
  ["60", "1/32"],
  ["40", "1/32 Triplet"],
];
const initial: QuantizeOptions = {
  target: "starts",
  mode: "yamaha",
  size: "120",
  soft: true,
  strength: 100,
  tuplet: 0,
  swing: 0,
  catchEnabled: false,
  catchPercent: 100,
  safeRange: 0,
  rough: 0,
  moveControllers: true,
  fromOriginal: false,
  sourceFiltered: false,
};
export function AdvancedQuantize({
  section,
  channel,
  notes,
  selected,
  view,
  disabled,
}: {
  section: Section;
  channel: number;
  notes: Note[];
  selected: number[];
  view: View;
  disabled: boolean;
}) {
  const [options, setOptions] = useState(initial),
    [auto, setAuto] = useState(false);
  const chosen = notes.filter((n) => selected.includes(n.id));
  const ids = chosen.map((n) => n.id),
    selectionKey = ids.join(",");
  const scopeKey = `${section.id}:${channel}`;
  const current = useRef({ section, channel, ids, disabled });
  current.current = { section, channel, ids, disabled };
  const patch = (next: Partial<QuantizeOptions>) =>
    setOptions((old) => ({ ...old, ...next }));
  useEffect(() => {
    if (!auto) return;
    let cancelled = false;
    const apply = () => {
      if (cancelled) return;
      const state = useStudio.getState(),
        c = current.current;
      if (state.busy) {
        timer = setTimeout(apply, 40);
        return;
      }
      if (c.disabled) return;
      void state.edit({
        type: "advancedQuantize",
        section: c.section.id,
        channel: c.channel,
        ids: c.ids,
        options: { ...options, fromOriginal: true },
      });
    };
    let timer = setTimeout(apply, 140);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [options, auto, scopeKey, selectionKey]);
  const execute = (reset = false) => {
    void useStudio.getState().edit({
      type: "advancedQuantize",
      section: section.id,
      channel,
      ids,
      options: { ...options, fromOriginal: false },
      reset,
    });
  };
  const baseline = view.quantizeOriginal?.[scopeKey];
  const mixed = options.size.startsWith("mixed");
  const number = (
    label: string,
    field: "strength" | "swing" | "catchPercent" | "safeRange" | "rough",
    max: number,
    inactive = false,
    min = 0,
  ) => (
    <ParameterControl
      label={label.replace(" percent", "").replace(" ticks", "")}
      ariaLabel={`Quantize ${label}`}
      min={min}
      max={max}
      value={Number(options[field])}
      disabled={disabled || inactive}
      resetValue={Number(initial[field])}
      format={(n) => `${n}${label.includes("ticks") ? " t" : "%"}`}
      onCommit={(n) => patch({ [field]: n })}
    />
  );
  const toggle = (
    label: string,
    ariaLabel: string,
    checked: boolean,
    onChange: (v: boolean) => void,
    inactive = false,
  ) => (
    <label className="aq-toggle">
      <input
        type="checkbox"
        aria-label={ariaLabel}
        checked={checked}
        disabled={disabled || inactive}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{label}</span>
    </label>
  );
  return (
    <div
      className="advanced-quantize"
      role="group"
      aria-label="Advanced quantize"
    >
      <fieldset className="aq-box">
        <legend>Grid</legend>
        <QuantizePreview options={options} />
        <div className="aq-row">
          <label className="aq-field">
            <span>Size</span>
            <select
              aria-label="Quantize size"
              value={options.size}
              disabled={disabled}
              onChange={(e) => {
                const size = e.target.value as QuantizeGrid;
                patch({ size });
                if (!size.startsWith("mixed")) {
                  useStudio.setState({ snap: 1920 / Number(size) });
                }
              }}
            >
              {quantizeGrids.map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="aq-field aq-narrow">
            <span>Tuplet</span>
            <select
              aria-label="Quantize tuplet"
              value={options.tuplet}
              disabled={disabled || mixed}
              onChange={(e) => patch({ tuplet: Number(e.target.value) })}
            >
              {[0, 3, 5, 7].map((n) => (
                <option key={n} value={n}>
                  {n || "Off"}
                </option>
              ))}
            </select>
          </label>
        </div>
      </fieldset>
      <fieldset className="aq-box">
        <legend>Affects</legend>
        <div className="aq-row">
          <label className="aq-field">
            <span>Target</span>
            <select
              aria-label="Quantize target"
              value={options.target}
              disabled={disabled}
              onChange={(e) =>
                patch({
                  target: e.target.value as QuantizeOptions["target"],
                  ...(e.target.value !== "starts"
                    ? { mode: "yamaha" as const }
                    : {}),
                })
              }
            >
              <option value="starts">Note starts</option>
              <option value="ends">Note ends</option>
              <option value="lengths">Note lengths</option>
              <option value="both">Starts and ends</option>
            </select>
          </label>
          <label className="aq-field">
            <span>Mode</span>
            <select
              aria-label="Quantize mode"
              disabled={disabled || options.target !== "starts"}
              value={options.mode}
              onChange={(e) =>
                patch({ mode: e.target.value as QuantizeOptions["mode"] })
              }
            >
              <option value="yamaha">Yamaha events · keep pattern</option>
              <option value="relative">Group offset</option>
            </select>
          </label>
        </div>
        {toggle(
          "Move controllers with note starts",
          "Move controllers with note starts",
          options.moveControllers,
          (v) => patch({ moveControllers: v }),
          options.target === "ends" || options.target === "lengths",
        )}
      </fieldset>
      <fieldset className="aq-box">
        <legend>Feel</legend>
        <div className="aq-toggles">
          {toggle(
            "Soft / Strength",
            "Quantize soft strength",
            options.soft,
            (v) => patch({ soft: v }),
          )}
          {toggle(
            "Catch range",
            "Quantize catch enabled",
            options.catchEnabled,
            (v) => patch({ catchEnabled: v }),
          )}
        </div>
        <div className="quantize-knobs">
          {number("strength", "strength", 100, !options.soft)}{" "}
          {number(
            "swing",
            "swing",
            100,
            mixed || options.tuplet > 0 || options.target === "lengths",
          )}
          {number("catch percent", "catchPercent", 100, !options.catchEnabled, 1)}
          {number("safe ticks", "safeRange", 960)}
          {number("rough ticks", "rough", 120)}
        </div>
      </fieldset>
      <div className="aq-actions">
        {toggle("Auto apply", "Quantize auto apply", auto, setAuto)}
        <button
          type="button"
          className="aq-reset"
          disabled={
            disabled || !(baseline?.notes.length || baseline?.events.length)
          }
          onClick={() => execute(true)}
        >
          Reset original
        </button>
        <button
          type="button"
          className="aq-apply"
          disabled={disabled || !notes.length}
          onClick={() => execute()}
        >
          {ids.length ? "Quantize selection" : "Quantize whole part"}
        </button>
      </div>
      <small>
        Timing changes keep pitch and velocity. Reset restores original timing,
        including moved CC and pitch bend.
      </small>
    </div>
  );
}
