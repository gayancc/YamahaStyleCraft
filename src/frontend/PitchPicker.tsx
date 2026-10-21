import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type WheelEvent } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { pitchName } from "./notePropertiesDraft";

const LETTER_SEMITONE: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
const MAX_PITCH = 127;
const OCTAVES_SHOWN = 2;
const WHITE_OFFSETS = [0, 2, 4, 5, 7, 9, 11];
/** Black keys: semitone offset in the octave and the white key (0-6) it sits to the right of. */
const BLACK_KEYS = [
  { offset: 1, after: 0 },
  { offset: 3, after: 1 },
  { offset: 6, after: 3 },
  { offset: 8, after: 4 },
  { offset: 10, after: 5 },
];

const clamp = (n: number) => Math.min(MAX_PITCH, Math.max(0, n));

/** Accepts "61", "C#4", "db3" or "A♯2" and returns a MIDI pitch, or null when it is not one. */
export function parsePitch(text: string): number | null {
  const raw = text.trim();
  if (/^\d{1,3}$/.test(raw)) {
    const n = Number(raw);
    return n <= MAX_PITCH ? n : null;
  }
  const m = /^([A-Ga-g])([#♯b♭]?)(-?\d)$/.exec(raw);
  if (!m) return null;
  const accidental = m[2] === "#" || m[2] === "♯" ? 1 : m[2] ? -1 : 0;
  const pitch = (Number(m[3]) + 1) * 12 + LETTER_SEMITONE[m[1].toUpperCase()] + accidental;
  return pitch >= 0 && pitch <= MAX_PITCH ? pitch : null;
}

const windowFor = (pitch: number, current: number) => {
  const maxBase = Math.floor(MAX_PITCH / 12) - OCTAVES_SHOWN + 1;
  if (pitch >= current * 12 && pitch < (current + OCTAVES_SHOWN) * 12) return current;
  return Math.min(maxBase, Math.floor(pitch / 12));
};

/** Pitch editor built around a mini piano: tap or glide across the keys, step the window by octave,
 * or type a note name / MIDI number in the big readout. */
export function PitchPicker({
  value,
  onChange,
}: {
  value: number;
  onChange: (pitch: number) => void;
}) {
  const [text, setText] = useState(pitchName(value));
  const [base, setBase] = useState(() => windowFor(value, 0));
  const dragging = useRef(false);
  useEffect(() => {
    setText(pitchName(value));
    setBase((b) => windowFor(value, b));
  }, [value]);

  const set = (pitch: number) => onChange(clamp(pitch));
  const maxBase = Math.floor(MAX_PITCH / 12) - OCTAVES_SHOWN + 1;
  const pitchAt = (x: number, y: number): number | null => {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-pitch]");
    return el ? Number(el.dataset.pitch) : null;
  };
  const press = (e: PointerEvent<HTMLDivElement>) => {
    const pitch = pitchAt(e.clientX, e.clientY);
    if (pitch === null) return;
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    if (pitch !== value) set(pitch);
  };
  const glide = (e: PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    const pitch = pitchAt(e.clientX, e.clientY);
    if (pitch !== null && pitch !== value) set(pitch);
  };
  const release = () => {
    dragging.current = false;
  };
  const onKey = (e: KeyboardEvent) => {
    const step = e.shiftKey ? 12 : 1;
    if (e.key === "ArrowUp") set(value + step);
    else if (e.key === "ArrowDown") set(value - step);
    else return;
    e.preventDefault();
  };
  const onWheel = (e: WheelEvent) => {
    if (document.activeElement !== e.currentTarget) return;
    set(value + (e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 12 : 1));
  };

  const whiteCount = OCTAVES_SHOWN * 7;
  const keyWidth = 100 / whiteCount;
  const octaves = Array.from({ length: OCTAVES_SHOWN }, (_, i) => base + i);
  const inRange = (pitch: number) => pitch <= MAX_PITCH;

  return (
    <div className="pitch-picker">
      <div className="pitch-readout">
        <button
          type="button"
          title="Show lower octaves"
          aria-label="Octaves down"
          disabled={base <= 0}
          onClick={() => setBase(base - 1)}
        >
          <ChevronLeft size={14} />
        </button>
        <input
          aria-label="Note properties pitch"
          title="Type C#4 or 61 · ↑/↓ or wheel = semitone · Shift = octave"
          spellCheck={false}
          value={text}
          onFocus={(e) => e.currentTarget.select()}
          onChange={(e) => {
            setText(e.target.value);
            const parsed = parsePitch(e.target.value);
            if (parsed !== null) onChange(parsed);
          }}
          onBlur={() => setText(pitchName(value))}
          onKeyDown={onKey}
          onWheel={onWheel}
        />
        <span className="pitch-midi" title="MIDI note number">
          MIDI
          <b>{value}</b>
        </span>
        <button
          type="button"
          title="Show higher octaves"
          aria-label="Octaves up"
          disabled={base >= maxBase}
          onClick={() => setBase(base + 1)}
        >
          <ChevronRight size={14} />
        </button>
      </div>
      <div
        className="pitch-piano"
        role="group"
        aria-label="Piano keys — click or drag to choose a pitch"
        onPointerDown={press}
        onPointerMove={glide}
        onPointerUp={release}
        onPointerCancel={release}
      >
        {octaves.flatMap((octave, oi) =>
          WHITE_OFFSETS.map((offset, wi) => {
            const pitch = octave * 12 + offset;
            return (
              <button
                type="button"
                tabIndex={-1}
                key={pitch}
                data-pitch={pitch}
                aria-label={pitchName(pitch)}
                disabled={!inRange(pitch)}
                className={`white${pitch === value ? " on" : ""}${
                  pitch % 12 === value % 12 && pitch !== value ? " same" : ""
                }`}
                style={{ left: `${(oi * 7 + wi) * keyWidth}%`, width: `${keyWidth}%` }}
              >
                {offset === 0 ? <span>C{octave - 1}</span> : null}
              </button>
            );
          }),
        )}
        {octaves.flatMap((octave, oi) =>
          BLACK_KEYS.map(({ offset, after }) => {
            const pitch = octave * 12 + offset;
            return (
              <button
                type="button"
                tabIndex={-1}
                key={pitch}
                data-pitch={pitch}
                aria-label={pitchName(pitch)}
                disabled={!inRange(pitch)}
                className={`black${pitch === value ? " on" : ""}`}
                style={{
                  left: `${(oi * 7 + after + 1) * keyWidth - keyWidth * 0.3}%`,
                  width: `${keyWidth * 0.6}%`,
                }}
              />
            );
          }),
        )}
      </div>
    </div>
  );
}
