import { useEffect, useState } from "react";
import { ParameterControl, formatPan, formatBipolar } from "./ParameterControl";
import type { Note, View } from "./contracts";
import { autoPanPoints, type AutoPanShape } from "./autoPan";
import { SAFE_FX_PRESETS, type SafeFxKey } from "./mixerPresets";
import { tickFromElapsed } from "./pianoEditing";

const roles = [
  "Rhythm 1",
  "Rhythm 2",
  "Bass",
  "Chord 1",
  "Chord 2",
  "Pad",
  "Phrase 1",
  "Phrase 2",
];
const colors = [
  "#b694c7",
  "#9e86ba",
  "#dfb46c",
  "#76aaa5",
  "#76a3b7",
  "#979ec5",
  "#b1b880",
  "#c58f83",
];

function role(ch: number): string {
  if (ch >= 9 && ch <= 16) return roles[ch - 9];
  return `CH ${ch}`;
}

/** Genos RPN pitch-bend sensitivity (SFM-PITCH-RANGE fingerprint). */
function BendRangeField({
  channel,
  value,
  disabled,
  onCommit,
}: {
  channel: number;
  value: number | null;
  disabled: boolean;
  onCommit: (n: number) => void;
}) {
  const shown = value ?? 2;
  const [v, set] = useState(shown);
  useEffect(() => set(shown), [shown]);
  return (
    <label
      className="mixer-slider"
      title="Standard MIDI RPN 0,0 (Genos/Genos2). Writes CC101/100/6 then null RPN at section start."
    >
      <span>Bend range · st</span>
      <input
        aria-label={`Ch ${channel} bend range semitones`}
        type="number"
        min={0}
        max={24}
        value={v}
        disabled={disabled}
        onChange={(e) => set(Math.max(0, Math.min(24, +e.target.value || 0)))}
        onBlur={() => {
          if (v !== value) onCommit(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur();
          }
        }}
      />
      <output>{value == null ? "?" : v}</output>
    </label>
  );
}

export type MixerPanelProps = {
  view: View;
  sectionId: number;
  sources: number[];
  muted: number[];
  solo: number[];
  armedChannel: number | null;
  mixerApplyAll: boolean;
  editingDisabled: boolean;
  capturing: boolean;
  busy: boolean;
  playheadTick: number | null;
  valueAt: (
    channel: number,
    status: number,
    controller: number,
    fallback: number,
  ) => number;
  /** Static Mixer EQ High/Low via NRPN (raw 0–127, 64 neutral). */
  eqValueAt: (channel: number, band: "eqLow" | "eqHigh") => number;
  onCc: (channel: number, controller: number, value: number) => void;
  onEq: (channel: number, band: "eqLow" | "eqHigh", value: number) => void;
  /** Replace section CC10 with Auto Pan curve (one undo). */
  onAutoPan: (
    channel: number,
    points: { tick: number; value: number }[],
    removeIds: number[],
  ) => void;
  /** Apply Safe FX preset CCs in one undo. */
  onSafeFx: (channel: number, changes: [number, number][]) => void;
  /** Genos RPN pitch-bend sensitivity (semitones 0–24) at section start. */
  onPitchBendRange: (channel: number, semitones: number) => void;
  /** Current bend-range semitones at section start, or null if unknown. */
  pitchBendRangeAt: (channel: number) => number | null;
  targetChannel: number;
  onMuteToggle: (channel: number) => void;
  onSoloToggle: (channel: number) => void;
  onArmToggle: (channel: number) => void;
  onMixerApplyAll: (next: boolean) => void;
  showArm?: boolean;
  onSelectChannel?: (channel: number) => void;
};

/** Shared mixer strips for main editor and detached Mixer (SFM-084). */
export function MixerPanel({
  view,
  sectionId,
  sources,
  muted,
  solo,
  armedChannel,
  mixerApplyAll,
  editingDisabled,
  capturing,
  busy,
  playheadTick,
  valueAt,
  eqValueAt,
  onCc,
  onEq,
  onAutoPan,
  onSafeFx,
  onPitchBendRange,
  pitchBendRangeAt,
  targetChannel,
  onMuteToggle,
  onSoloToggle,
  onArmToggle,
  onMixerApplyAll,
  showArm = true,
  onSelectChannel,
}: MixerPanelProps) {
  const section = view.sections.find((s) => s.id === sectionId) ?? null;
  const [bank, setBank] = useState<"sends" | "tone" | "performance">("sends");
  const [panBeats, setPanBeats] = useState(1);
  const [panShape, setPanShape] = useState<AutoPanShape>("sine");
  const [panDepth, setPanDepth] = useState(50);
  const [panReverse, setPanReverse] = useState(false);
  const [safeFx, setSafeFx] = useState<SafeFxKey>("tight");
  const panTarget = sources.includes(targetChannel)
    ? targetChannel
    : (sources[0] ?? targetChannel);

  const existingPanIds = () => {
    if (!section) return [] as number[];
    return view.events
      .filter(
        (e) =>
          e.tick >= section.start &&
          e.tick < section.end &&
          e.data.length === 3 &&
          e.data[0] === (0xb0 | (panTarget - 1)) &&
          e.data[1] === 10,
      )
      .map((e) => e.id);
  };

  const applyAutoPan = () => {
    if (!section || editingDisabled) return;
    const lengthTicks = Math.max(1, section.end - section.start);
    const points = autoPanPoints({
      lengthTicks,
      ppq: view.ppq,
      beats: panBeats,
      shape: panShape,
      depth: panDepth,
      reverse: panReverse,
      sectionStart: section.start,
    });
    onAutoPan(panTarget, points, existingPanIds());
  };

  const removeAutoPan = () => {
    if (!section || editingDisabled) return;
    const removeIds = existingPanIds();
    onAutoPan(panTarget, [{ tick: section.start, value: 64 }], removeIds);
  };

  return (
    <div className="mixer" key={sectionId}>
      <div className="mixer-console-heading">
        <div>
          <h3>Part mixer</h3>
          <span className="muted">{section?.label} · MIDI levels 0–127</span>
        </div>
        <div
          className="console-banks"
          role="group"
          aria-label="Mixer parameter bank"
        >
          {(["sends", "tone", "performance"] as const).map((b) => (
            <button
              type="button"
              key={b}
              aria-pressed={bank === b}
              className={bank === b ? "active" : ""}
              onClick={() => setBank(b)}
            >
              {b === "sends"
                ? "Sends"
                : b === "tone"
                  ? "Tone / EQ"
                  : "Performance"}
            </button>
          ))}
        </div>
      </div>
      <label className="mixer-scope">
        <input
          type="checkbox"
          checked={mixerApplyAll}
          disabled={editingDisabled}
          onChange={(e) => onMixerApplyAll(e.target.checked)}
        />
        Apply mixer CC to all sections
      </label>
      <details className="mixer-utilities">
        <summary>Part tools · {role(panTarget)}</summary>
        <div className="mixer-utilities-body">
          <fieldset
            className="mixer-autopan"
            disabled={editingDisabled || !section}
          >
            <legend>Safe FX · CH {panTarget}</legend>
            <label>
              Preset
              <select
                aria-label="Yamaha Safe FX preset"
                value={safeFx}
                onChange={(e) => setSafeFx(e.target.value as SafeFxKey)}
              >
                {SAFE_FX_PRESETS.map((p) => (
                  <option key={p.key} value={p.key}>
                    {p.label}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              disabled={busy || !section}
              onClick={() => {
                const preset = SAFE_FX_PRESETS.find((p) => p.key === safeFx);
                if (!preset) return;
                onSafeFx(
                  panTarget,
                  preset.values.map((v) => [v.controller, v.value]),
                );
              }}
            >
              Apply Safe FX
            </button>
            <p
              className="muted mixer-note"
              style={{ margin: 0, flexBasis: "100%" }}
            >
              MIDI CC only (reverb/chorus/cutoff/resonance). Does not rewrite
              DSP SysEx.
            </p>
          </fieldset>
          <fieldset
            className="mixer-autopan"
            disabled={editingDisabled || !section}
          >
            <legend>Auto Pan · CH {panTarget}</legend>
            <label>
              Beats
              <input
                type="number"
                min={1}
                max={16}
                value={panBeats}
                aria-label="Auto Pan beats"
                onChange={(e) =>
                  setPanBeats(Math.max(1, Math.min(16, +e.target.value || 1)))
                }
              />
            </label>
            <label>
              Shape
              <select
                aria-label="Auto Pan shape"
                value={panShape}
                onChange={(e) => setPanShape(e.target.value as AutoPanShape)}
              >
                <option value="sine">Sine</option>
                <option value="triangle">Triangle</option>
                <option value="square">Square</option>
              </select>
            </label>
            <ParameterControl
              label="Depth"
              ariaLabel="Auto Pan depth"
              value={panDepth}
              max={100}
              resetValue={50}
              disabled={editingDisabled}
              format={(n) => `${n}%`}
              onCommit={setPanDepth}
            />
            <label className="mixer-scope">
              <input
                type="checkbox"
                checked={panReverse}
                onChange={(e) => setPanReverse(e.target.checked)}
              />
              Reverse
            </label>
            <div className="mixer-autopan-actions">
              <button
                type="button"
                disabled={busy || !section}
                onClick={applyAutoPan}
              >
                Apply
              </button>
              <button
                type="button"
                disabled={busy || !section}
                onClick={removeAutoPan}
              >
                Remove
              </button>
            </div>
          </fieldset>
        </div>
      </details>
      <div className="mixer-strips">
        {sources.map((ch) => {
          const sounding =
            playheadTick == null
              ? []
              : view.notes.filter(
                  (n: Note) =>
                    n.channel === ch &&
                    n.start <= playheadTick &&
                    playheadTick < n.start + n.duration,
                );
          const maxVel = sounding.reduce((m, n) => Math.max(m, n.velocity), 0);
          // SFM-080: 4-segment activity (idle / low / mid / peak).
          const level = maxVel <= 0 ? 0 : maxVel < 48 ? 1 : maxVel < 96 ? 2 : 3;
          return (
            <div
              className={`mixer-strip${ch === targetChannel ? " selected" : ""}`}
              key={ch}
              onPointerDownCapture={() => {
                if (ch !== targetChannel) onSelectChannel?.(ch);
              }}
              onFocusCapture={() => {
                if (ch !== targetChannel) onSelectChannel?.(ch);
              }}
            >
              <div
                className="mixer-track-color"
                style={{ background: colors[(ch - 9 + 16) % 8] }}
              />
              <span
                className={`mixer-activity level-${level}`}
                aria-hidden
                title={
                  level === 0
                    ? "No note at playhead"
                    : level === 3
                      ? "Peak activity at playhead"
                      : "Source sounding at playhead"
                }
              />
              <button
                type="button"
                className="mixer-part-name"
                aria-label={`Select ${role(ch)}`}
                aria-pressed={ch === targetChannel}
                onClick={() => onSelectChannel?.(ch)}
              >
                {role(ch)}
              </button>
              <span>CH {ch}</span>
              <div className="mute-solo">
                <button
                  type="button"
                  className={muted.includes(ch) ? "active" : ""}
                  title={`Mute CH${ch} (playback)`}
                  aria-label={`Mute CH${ch}`}
                  aria-pressed={muted.includes(ch)}
                  onClick={() => onMuteToggle(ch)}
                >
                  M
                </button>
                <button
                  type="button"
                  className={solo.includes(ch) ? "active" : ""}
                  title={`Solo CH${ch} (playback)`}
                  aria-label={`Solo CH${ch}`}
                  aria-pressed={solo.includes(ch)}
                  onClick={() => onSoloToggle(ch)}
                >
                  S
                </button>
                {showArm && (
                  <button
                    type="button"
                    className={armedChannel === ch ? "active" : ""}
                    title={`Arm CH${ch} for recording`}
                    aria-label={`Arm CH${ch} for recording`}
                    aria-pressed={armedChannel === ch}
                    disabled={capturing || busy}
                    onClick={() => onArmToggle(ch)}
                  >
                    R
                  </button>
                )}
              </div>
              <fieldset disabled={editingDisabled} className="console-controls">
                <ParameterControl
                  label="Pan"
                  ariaLabel={`Ch ${ch} pan`}
                  value={valueAt(ch, 0xb0, 10, 64)}
                  resetValue={64}
                  bipolar
                  format={formatPan}
                  disabled={editingDisabled}
                  onCommit={(n) => onCc(ch, 10, n)}
                />
                <div className="mixer-bank-controls">
                  {bank === "sends" && (
                    <>
                      <ParameterControl
                        label="Reverb"
                        ariaLabel={`Ch ${ch} reverb send`}
                        value={valueAt(ch, 0xb0, 91, 40)}
                        resetValue={40}
                        disabled={editingDisabled}
                        onCommit={(n) => onCc(ch, 91, n)}
                      />
                      <ParameterControl
                        label="Chorus"
                        ariaLabel={`Ch ${ch} chorus send`}
                        value={valueAt(ch, 0xb0, 93, 0)}
                        disabled={editingDisabled}
                        onCommit={(n) => onCc(ch, 93, n)}
                      />
                    </>
                  )}
                  {bank === "tone" && (
                    <>
                      <ParameterControl
                        label="Cutoff"
                        ariaLabel={`Ch ${ch} cutoff`}
                        value={valueAt(ch, 0xb0, 74, 64)}
                        resetValue={64}
                        bipolar
                        format={formatBipolar}
                        disabled={editingDisabled}
                        onCommit={(n) => onCc(ch, 74, n)}
                      />
                      <ParameterControl
                        label="Resonance"
                        ariaLabel={`Ch ${ch} resonance`}
                        value={valueAt(ch, 0xb0, 71, 64)}
                        resetValue={64}
                        bipolar
                        format={formatBipolar}
                        disabled={editingDisabled}
                        onCommit={(n) => onCc(ch, 71, n)}
                      />
                      <ParameterControl
                        label="EQ Low"
                        ariaLabel={`Ch ${ch} EQ Low`}
                        value={eqValueAt(ch, "eqLow")}
                        resetValue={64}
                        bipolar
                        format={formatBipolar}
                        disabled={editingDisabled}
                        onCommit={(n) => onEq(ch, "eqLow", n)}
                      />
                      <ParameterControl
                        label="EQ High"
                        ariaLabel={`Ch ${ch} EQ High`}
                        value={eqValueAt(ch, "eqHigh")}
                        resetValue={64}
                        bipolar
                        format={formatBipolar}
                        disabled={editingDisabled}
                        onCommit={(n) => onEq(ch, "eqHigh", n)}
                      />
                    </>
                  )}
                  {bank === "performance" && (
                    <>
                      <ParameterControl
                        label="Expression"
                        ariaLabel={`Ch ${ch} expression`}
                        value={valueAt(ch, 0xb0, 11, 127)}
                        resetValue={127}
                        disabled={editingDisabled}
                        onCommit={(n) => onCc(ch, 11, n)}
                      />
                      <div className="sustain-control">
                        <button
                          type="button"
                          role="switch"
                          aria-checked={valueAt(ch, 0xb0, 64, 0) >= 64}
                          aria-label={`Ch ${ch} sustain`}
                          disabled={editingDisabled}
                          onClick={() =>
                            onCc(
                              ch,
                              64,
                              valueAt(ch, 0xb0, 64, 0) >= 64 ? 0 : 127,
                            )
                          }
                        >
                          Sustain{" "}
                          <span>
                            {valueAt(ch, 0xb0, 64, 0) >= 64 ? "On" : "Off"}
                          </span>
                        </button>
                        <label>
                          CC64
                          <input
                            aria-label={`Ch ${ch} sustain value`}
                            type="number"
                            min={0}
                            max={127}
                            key={valueAt(ch, 0xb0, 64, 0)}
                            defaultValue={valueAt(ch, 0xb0, 64, 0)}
                            disabled={editingDisabled}
                            onBlur={(e) => {
                              const v = Number(e.target.value);
                              if (
                                e.target.value.trim() &&
                                Number.isInteger(v) &&
                                v >= 0 &&
                                v <= 127 &&
                                v !== valueAt(ch, 0xb0, 64, 0)
                              )
                                onCc(ch, 64, v);
                              else
                                e.target.value = String(
                                  valueAt(ch, 0xb0, 64, 0),
                                );
                            }}
                            onKeyDown={(e) => {
                              e.stopPropagation();
                              if (e.key === "Enter") e.currentTarget.blur();
                            }}
                          />
                        </label>
                      </div>
                      <BendRangeField
                        channel={ch}
                        value={pitchBendRangeAt(ch)}
                        disabled={editingDisabled}
                        onCommit={(n) => onPitchBendRange(ch, n)}
                      />
                    </>
                  )}
                </div>
                <ParameterControl
                  variant="fader"
                  label="Volume"
                  ariaLabel={`Ch ${ch} volume`}
                  value={valueAt(ch, 0xb0, 7, 100)}
                  resetValue={100}
                  disabled={editingDisabled}
                  onCommit={(n) => onCc(ch, 7, n)}
                />
              </fieldset>
            </div>
          );
        })}
      </div>
      {section && (
        <p className="muted mixer-note">
          Section {section.label} · {sources.length} sources
        </p>
      )}
    </div>
  );
}

export function mixerPlayheadTick(
  view: View | null,
  sectionId: number,
  playing: boolean,
  elapsedMs: number,
): number | null {
  if (!view || !playing) return null;
  const section = view.sections.find((s) => s.id === sectionId);
  if (!section) return null;
  return tickFromElapsed(view, section, elapsedMs);
}

/** Read Genos RPN pitch-bend sensitivity (semitones) at or before `atTick`. */
export function pitchBendRangeSemitones(
  events: { tick: number; order: number; id: number; data: number[] }[],
  channel: number,
  atTick: number,
): number | null {
  if (!(channel >= 1 && channel <= 16)) return null;
  const status = 0xb0 | (channel - 1);
  const rows = events
    .filter(
      (e) => e.tick <= atTick && e.data.length === 3 && e.data[0] === status,
    )
    .sort((a, b) => a.tick - b.tick || a.order - b.order || a.id - b.id);
  let selected: [number | null, number | null] = [null, null];
  let mode: "rpn" | "nrpn" | null = null;
  let semitones: number | null = null;
  for (const e of rows) {
    const controller = e.data[1];
    const value = e.data[2];
    if (controller === 121) {
      selected = [null, null];
      mode = null;
      semitones = null;
    } else if (controller === 101 || controller === 100) {
      selected[controller === 101 ? 0 : 1] = value;
      mode = "rpn";
    } else if (controller === 99 || controller === 98) {
      mode = "nrpn";
    } else if (
      controller === 6 &&
      mode === "rpn" &&
      selected[0] === 0 &&
      selected[1] === 0
    ) {
      semitones = value >= 0 && value <= 24 ? value : null;
    } else if (
      (controller === 96 || controller === 97) &&
      mode === "rpn" &&
      selected[0] === 0 &&
      selected[1] === 0
    ) {
      semitones = null;
    }
  }
  return semitones;
}

/** Read static Mixer EQ High/Low NRPN at section start (raw 0–127, 64 neutral). */
export function eqValueAt(
  view: View | null,
  sectionId: number,
  channel: number,
  band: "eqLow" | "eqHigh",
): number {
  if (!view) return 64;
  const section = view.sections.find((s) => s.id === sectionId);
  if (!section) return 64;
  const status = 0xb0 | (channel - 1);
  const lsb = band === "eqLow" ? 48 : 49;
  const atStart = view.events
    .filter((e) => e.tick === section.start && e.data[0] === status)
    .sort((a, b) => a.order - b.order || a.id - b.id);
  let msb: number | null = null;
  let nrpnLsb: number | null = null;
  let found: number | null = null;
  for (const e of atStart) {
    if (e.data.length !== 3) continue;
    const controller = e.data[1];
    const v = e.data[2];
    if (controller === 99) {
      msb = v;
      nrpnLsb = null;
    } else if (controller === 98) {
      nrpnLsb = v;
    } else if (controller === 6) {
      if (msb === 1 && nrpnLsb === lsb) found = v;
      msb = null;
      nrpnLsb = null;
    } else if (controller === 100 || controller === 101) {
      msb = null;
      nrpnLsb = null;
    }
  }
  return found ?? 64;
}
