import { useState } from "react";
import type { MidiEvent, Note, Section, View } from "./contracts";
import {
  NOTE_FX_CONTROLLERS,
  buildNoteFxEnvelope,
  mergeNoteFxRanges,
  noteFxClearIds,
  type NoteFxCurve,
  type NoteFxSettings,
} from "./noteFx";

export function NoteFxPanel({
  view,
  section,
  channel,
  notes,
  disabled,
  onApply,
  onClear,
}: {
  view: View;
  section: Section;
  channel: number;
  notes: Note[];
  disabled: boolean;
  onApply: (args: {
    controller: number;
    points: { tick: number; value: number }[];
    remove: number[];
  }) => void;
  onClear: (args: { controller: number; remove: number[] }) => void;
}) {
  const [controller, setController] = useState(91);
  const [startValue, setStart] = useState(24);
  const [endValue, setEnd] = useState(72);
  const [curve, setCurve] = useState<NoteFxCurve>("linear");
  const [restore, setRestore] = useState(true);
  const enabled = !disabled && notes.length > 0;
  const isSwitch = controller === 64;
  const status = 0xb0 | (channel - 1);
  const channelCc = (e: MidiEvent) =>
    e.data.length === 3 && e.data[0] === status;

  const apply = () => {
    if (!enabled) return;
    const settings: NoteFxSettings = {
      controller,
      startValue,
      endValue,
      curve: isSwitch ? "step" : curve,
      restore,
    };
    const ranges = mergeNoteFxRanges(notes, section.end);
    const baseline = view.events.filter(
      (e) => channelCc(e) && e.data[1] === controller,
    );
    const remove = noteFxClearIds(
      view.events,
      channel,
      controller,
      ranges,
      section.end,
      settings.restore,
    );
    const points = ranges.flatMap((range) =>
      buildNoteFxEnvelope(
        range,
        settings,
        baseline.filter((e) => !remove.includes(e.id)),
        view.ppq,
        section.end,
      ),
    );
    // Deduplicate ticks (later wins).
    const byTick = new Map<number, number>();
    for (const p of points) byTick.set(p.tick, p.value);
    onApply({
      controller,
      points: [...byTick].map(([tick, value]) => ({ tick, value })),
      remove,
    });
  };

  const clear = () => {
    if (!enabled) return;
    const ranges = mergeNoteFxRanges(notes, section.end);
    // Include possible restore ticks from a prior apply.
    const remove = noteFxClearIds(
      view.events,
      channel,
      controller,
      ranges,
      section.end,
      true,
    );
    if (remove.length) onClear({ controller, remove });
  };

  return (
    <div className="note-fx-panel">
      <h3>Note effects</h3>
      <p className="muted">
        Apply a CC envelope across the selected notes (plain MIDI). Restore
        writes the prior value at note end. Overlapping notes merge into one
        span.
      </p>
      <label className="field">
        <span>Effect</span>
        <select
          aria-label="Note effect controller"
          value={controller}
          disabled={!enabled}
          onChange={(e) => setController(+e.target.value)}
        >
          {NOTE_FX_CONTROLLERS.map(([id, name]) => (
            <option key={id} value={id}>
              {name}
            </option>
          ))}
        </select>
      </label>
      <label className="field">
        <span>Start</span>
        <input
          aria-label="Note effect start value"
          type="number"
          min={0}
          max={127}
          value={startValue}
          disabled={!enabled}
          onChange={(e) => setStart(+e.target.value)}
        />
      </label>
      <label className="field">
        <span>End</span>
        <input
          aria-label="Note effect end value"
          type="number"
          min={0}
          max={127}
          value={endValue}
          disabled={!enabled}
          onChange={(e) => setEnd(+e.target.value)}
        />
      </label>
      <label className="field">
        <span>Curve</span>
        <select
          aria-label="Note effect curve"
          value={curve}
          disabled={!enabled || isSwitch}
          onChange={(e) => setCurve(e.target.value as NoteFxCurve)}
        >
          <option value="linear">Linear</option>
          <option value="ease-in">Ease in</option>
          <option value="ease-out">Ease out</option>
          <option value="step">Step</option>
        </select>
      </label>
      <label className="mixer-scope">
        <input
          type="checkbox"
          checked={restore}
          disabled={!enabled}
          onChange={(e) => setRestore(e.target.checked)}
        />
        Restore after note
      </label>
      <div className="modal-actions" style={{ marginTop: 8 }}>
        <button type="button" disabled={!enabled} onClick={apply}>
          Apply to selected notes
        </button>
        <button type="button" disabled={!enabled} onClick={clear}>
          Clear FX in note span
        </button>
      </div>
      <p className="muted" role="status">
        {!notes.length
          ? "Select one or more notes to edit their effect envelope."
          : isSwitch
            ? `${notes.length} selected · Sustain uses OFF (0) / ON (127).`
            : `${notes.length} selected · ready for CC${controller}.`}
      </p>
    </div>
  );
}
