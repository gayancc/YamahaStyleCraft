import { restoreEditorFocus } from "./modal";
import { useEffect, useRef } from "react";
import type { Port } from "./contracts";
import { trapModalFocus } from "./modal";
import { clickMetronome } from "./metronome";

/**
 * Dedicated Recording & MIDI settings dialog (SFM-044 / SFM-MENU-SETUP-REC-*).
 * Genos EQ input is excluded — no fingerprinted part-EQ SysEx path.
 */
export type RecDataMode = "all" | "notes" | "controllers";

export function RecordingMidiDialog({
  inputs,
  outputs,
  inputName,
  output,
  countInBars,
  recQuantize,
  midiThru,
  recordingOffsetMs,
  recData,
  recInputChannel,
  recordVoice,
  recordBankMsb,
  recordBankLsb,
  recordProgram,
  metronome,
  ppq,
  disabled,
  onClose,
  onInputChange,
  onOutputChange,
  onCountInChange,
  onQuantizeChange,
  onThruChange,
  onOffsetChange,
  onRecDataChange,
  onInputChannelChange,
  onRecordVoiceChange,
  onBankMsbChange,
  onBankLsbChange,
  onProgramChange,
  onMetronomeChange,
}: {
  inputs: Port[];
  outputs: Port[];
  inputName: string;
  output: Port | null;
  countInBars: number;
  recQuantize: number;
  midiThru: boolean;
  recordingOffsetMs: number;
  recData: RecDataMode;
  /** 0 = OMNI, 1–16 = MIDI channel filter. */
  recInputChannel: number;
  recordVoice: boolean;
  recordBankMsb: number;
  recordBankLsb: number;
  recordProgram: number;
  metronome: boolean;
  ppq: number;
  disabled: boolean;
  onClose: () => void;
  onInputChange: (name: string) => void;
  onOutputChange: (port: Port | null) => void;
  onCountInChange: (n: number) => void;
  onQuantizeChange: (n: number) => void;
  onThruChange: (v: boolean) => void;
  onOffsetChange: (n: number) => void;
  onRecDataChange: (mode: RecDataMode) => void;
  onInputChannelChange: (ch: number) => void;
  onRecordVoiceChange: (v: boolean) => void;
  onBankMsbChange: (n: number) => void;
  onBankLsbChange: (n: number) => void;
  onProgramChange: (n: number) => void;
  onMetronomeChange: (v: boolean) => void;
}) {
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button,select,input")?.focus();
    return () => restoreEditorFocus(prior);
  }, []);

  return (
    <div
      className="modal-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget && !disabled) close.current();
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" && !disabled) {
          e.preventDefault();
          close.current();
        }
      }}
    >
      <section
        ref={dialog}
        className="modal recording-midi-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="recording-midi-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={trapModalFocus}
      >
        <div className="modal-title">
          <h2 id="recording-midi-title">Recording &amp; MIDI</h2>
          <button
            type="button"
            aria-label="Close"
            disabled={disabled}
            onClick={() => close.current()}
          >
            ×
          </button>
        </div>
        <div className="recording-midi-body">
          <p className="source-zone">
            Capture from the MIDI panel or transport Rec. Genos part-EQ input is
            unavailable (no verified SysEx path).
          </p>
          <fieldset className="fl-plain" disabled={disabled}>
            <fieldset className="fl-section fl-rows">
              <legend>Input</legend>
              <label>
                <span>Input</span>
                <select
                  aria-label="Recording MIDI input"
                  value={inputName}
                  onChange={(e) => onInputChange(e.target.value)}
                >
                  <option value="">No input selected</option>
                  {inputs.map((p) => (
                    <option key={`${p.index}:${p.name}`} value={p.name}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>MIDI output / sound source</span>
                <select
                  aria-label="Recording MIDI output"
                  value={output ? `${output.index}:${output.name}` : ""}
                  onChange={(e) => {
                    const port =
                      outputs.find(
                        (p) => `${p.index}:${p.name}` === e.target.value,
                      ) ?? null;
                    onOutputChange(port);
                  }}
                >
                  <option value="">No output selected</option>
                  {outputs.map((p) => (
                    <option
                      key={`${p.index}:${p.name}`}
                      value={`${p.index}:${p.name}`}
                    >
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>Channel</span>
                <select
                  aria-label="Recording MIDI input channel"
                  value={recInputChannel}
                  onChange={(e) => onInputChannelChange(+e.target.value)}
                >
                  <option value={0}>OMNI (all channels → armed part)</option>
                  {Array.from({ length: 16 }, (_, i) => i + 1).map((ch) => (
                    <option key={ch} value={ch}>
                      Channel {ch}
                    </option>
                  ))}
                </select>
              </label>
            </fieldset>
            <fieldset className="fl-section fl-rows">
              <legend>Timing</legend>
              <label>
                <span>Count-in</span>
                <select
                  aria-label="Capture count-in bars"
                  value={countInBars}
                  onChange={(e) => onCountInChange(+e.target.value)}
                >
                  {[0, 1, 2, 3, 4].map((n) => (
                    <option key={n} value={n}>
                      {n === 0 ? "Off" : `${n} bar${n > 1 ? "s" : ""}`}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>Rec quantize</span>
                <select
                  aria-label="Capture apply quantize"
                  value={recQuantize}
                  onChange={(e) => onQuantizeChange(+e.target.value)}
                >
                  <option value={0}>Off</option>
                  {[
                    ppq,
                    Math.round(ppq / 2),
                    Math.round(ppq / 4),
                    Math.round(ppq / 8),
                    Math.round(ppq / 16),
                  ]
                    .filter((n, i, a) => n > 0 && a.indexOf(n) === i)
                    .map((n) => (
                      <option key={n} value={n}>
                        {n} ticks
                      </option>
                    ))}
                </select>
              </label>
              <label>
                <span>Recording offset (ms)</span>
                <input
                  type="number"
                  aria-label="Recording offset ms"
                  min={-2000}
                  max={2000}
                  step={1}
                  value={recordingOffsetMs}
                  onChange={(e) => {
                    const next = Math.min(
                      2000,
                      Math.max(-2000, Number(e.target.value) || 0),
                    );
                    onOffsetChange(next);
                  }}
                  title="Positive pulls notes earlier (latency compensation)"
                />
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={midiThru}
                  disabled={!output}
                  onChange={(e) => onThruChange(e.target.checked)}
                />
                <span>MIDI thru</span>
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={metronome}
                  onChange={(e) => onMetronomeChange(e.target.checked)}
                />
                <span>Metronome during playback</span>
              </label>
              <div className="capture-options">
                <button
                  type="button"
                  disabled={disabled}
                  title="PC-audio click (not Genos MIDI) — SFM Test click"
                  aria-label="Test metronome click"
                  onClick={() => clickMetronome(true)}
                >
                  Test click
                </button>
                <span className="muted">
                  Click uses computer audio, not MIDI out
                </span>
              </div>
            </fieldset>
            <fieldset className="fl-section fl-rows">
              <legend>Record data</legend>
              <label>
                <span>Record data</span>
                <select
                  aria-label="Record data type"
                  value={recData}
                  onChange={(e) =>
                    onRecDataChange(e.target.value as RecDataMode)
                  }
                >
                  <option value="all">Notes + controllers + Voice</option>
                  <option value="controllers">Controllers + Voice only</option>
                  <option value="notes">Notes only</option>
                </select>
              </label>
              <label
                className="check"
                title={
                  recData === "notes"
                    ? "Voice change applies with controller/Voice Record data modes"
                    : undefined
                }
              >
                <input
                  type="checkbox"
                  checked={recordVoice}
                  disabled={recData === "notes"}
                  onChange={(e) => onRecordVoiceChange(e.target.checked)}
                />
                <span>Record Voice change</span>
              </label>
              {recordVoice && recData !== "notes" && (
                <div className="capture-options">
                  <label>
                    Bank MSB{" "}
                    <input
                      aria-label="Record voice bank MSB"
                      type="number"
                      min={0}
                      max={127}
                      value={recordBankMsb}
                      onChange={(e) =>
                        onBankMsbChange(
                          Math.max(0, Math.min(127, +e.target.value || 0)),
                        )
                      }
                    />
                  </label>
                  <label>
                    Bank LSB{" "}
                    <input
                      aria-label="Record voice bank LSB"
                      type="number"
                      min={0}
                      max={127}
                      value={recordBankLsb}
                      onChange={(e) =>
                        onBankLsbChange(
                          Math.max(0, Math.min(127, +e.target.value || 0)),
                        )
                      }
                    />
                  </label>
                  <label>
                    Program{" "}
                    <input
                      aria-label="Record voice program"
                      type="number"
                      min={1}
                      max={128}
                      value={recordProgram}
                      onChange={(e) =>
                        onProgramChange(
                          Math.max(1, Math.min(128, +e.target.value || 1)),
                        )
                      }
                    />
                  </label>
                </div>
              )}
            </fieldset>
          </fieldset>
        </div>
        <div className="modal-actions">
          <button
            type="button"
            disabled={disabled}
            onClick={() => close.current()}
          >
            Done
          </button>
        </div>
      </section>
    </div>
  );
}
