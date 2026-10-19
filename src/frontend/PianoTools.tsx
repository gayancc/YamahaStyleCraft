import { useEffect, useMemo, useState } from "react";
import type { RefObject } from "react";
import type { Note, Section, View } from "./contracts";
import { useStudio } from "./store";
import { casmAdvisory, clamp, transformNotes } from "./pianoEditing";
import type { QuantizeTarget, Transform } from "./pianoEditing";
import { DEFAULT_NOTE_COLORS, sanitizeNoteColors } from "./noteColors";

const operations: [Transform, string][] = [
  ["quantize", "Quantize"],
  ["legato", "Legato"],
  ["gate", "Articulate / gate %"],
  ["chop", "Chop at grid"],
  ["glue", "Glue touching notes"],
  ["strum", "Strum · ticks/note"],
  ["flam", "Flam · lead-in ticks"],
  ["drumRoll", "Drum Roll · spacing ticks"],
  ["stretch", "Stretch timing %"],
  ["fitToBars", "Fit to bars"],
  ["beatRepeat", "Beat Repeat · period ticks"],
  ["flipTime", "Flip time"],
  ["flipPitch", "Flip pitch"],
  ["limit", "Limit pitch"],
  ["velocityScale", "Scale velocity %"],
  ["velocityRandom", "Randomize velocity ±"],
];
export function PianoTools({
  notes,
  section,
  view,
  channel,
  grid,
  disabled,
  row,
  onRow,
  length,
  onLength,
  ghosts,
  onGhosts,
  follow,
  onFollow,
  scroll,
  width,
  noteColors,
  onNoteColorsChange,
}: {
  notes: Note[];
  section: Section;
  view: View;
  channel: number;
  grid: number;
  disabled: boolean;
  row: number;
  onRow: (n: number) => void;
  length: number;
  onLength: (n: number) => void;
  ghosts: boolean;
  onGhosts: (v: boolean) => void;
  follow: boolean;
  onFollow: (v: boolean) => void;
  scroll: RefObject<HTMLDivElement | null>;
  width: number;
  noteColors?: string[];
  onNoteColorsChange?: (colors: string[]) => void;
}) {
  const colors = useMemo(
    () => sanitizeNoteColors(noteColors ?? DEFAULT_NOTE_COLORS),
    [noteColors],
  );
  const { selected, edit } = useStudio();
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const chosen = notes.filter((n) => selectedSet.has(n.id));
  const [operation, setOperation] = useState<Transform>("quantize"),
    [amount, setAmount] = useState(100),
    [strength, setStrength] = useState(100),
    [duration, setDuration] = useState(false),
    [moveControllers, setMoveControllers] = useState(true),
    [swing, setSwing] = useState(0),
    [tuplet, setTuplet] = useState(0),
    [quantizeTarget, setQuantizeTarget] =
      useState<QuantizeTarget>("starts"),
    [catchRange, setCatchRange] = useState(0),
    [safeRange, setSafeRange] = useState(0),
    [roughTicks, setRoughTicks] = useState(0),
    [low, setLow] = useState(36),
    [high, setHigh] = useState(84);
  const advisory = casmAdvisory(
    view,
    section,
    channel,
    chosen.length ? chosen : notes,
  );
  const fit = () => {
    const ns = chosen.length ? chosen : notes,
      el = scroll.current;
    if (!ns.length || !el) return;
    const first = Math.min(...ns.map((n) => n.start)),
      last = Math.max(...ns.map((n) => n.start + n.duration));
    const zoom = clamp(
      ((width - 110) * view.ppq) / (72 * Math.max(view.ppq / 4, last - first)),
      0.125,
      8,
    );
    useStudio.setState({ zoom });
    requestAnimationFrame(() => {
      el.scrollLeft = Math.max(
        0,
        ((first - section.start) * 72 * zoom) / view.ppq - 20,
      );
      el.scrollTop = Math.max(
        0,
        (127 - Math.max(...ns.map((n) => n.pitch))) * row - 40,
      );
    });
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (
        disabled ||
        document.querySelector('[aria-modal="true"]') ||
        (e.target as HTMLElement).closest(
          'input,select,textarea,[contenteditable="true"]',
        )
      )
        return;
      if (e.shiftKey && e.key === "5") {
        e.preventDefault();
        fit();
      }
      if (e.key === "PageUp" || e.key === "PageDown") {
        e.preventDefault();
        const el = scroll.current;
        if (!el) return;
        const old = useStudio.getState().zoom,
          at = ((el.scrollLeft + width / 2 - 60) * view.ppq) / (72 * old),
          next = clamp(old * (e.key === "PageUp" ? 1.5 : 1 / 1.5), 0.125, 8);
        useStudio.setState({ zoom: next });
        requestAnimationFrame(() => {
          el.scrollLeft = Math.max(
            0,
            (at * 72 * next) / view.ppq - width / 2 + 60,
          );
        });
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [disabled, notes, selected, width, view, row, section]);
  const apply = () => {
    try {
      void edit(
        transformNotes(operation, chosen, section, channel, {
          step: grid,
          amount,
          strength,
          quantizeDuration: duration,
          moveControllers,
          swing,
          tuplet,
          quantizeTarget,
          catchRange,
          safeRange,
          roughTicks,
          low,
          high,
          seed: Date.now(),
          barTicks: Math.round(
            ((view.ppq * 4) / view.denominator) * view.numerator,
          ),
        }),
      );
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const lengths = [
    ...new Set(
      [view.ppq / 4, view.ppq / 2, view.ppq, view.ppq * 2, length]
        .filter((n) => n > 0)
        .map((n) => Math.max(1, Math.round(n))),
    ),
  ].sort((a, b) => a - b);
  return (
    <div className="piano-options">
      <label>
        Length{" "}
        <select
          aria-label="New note length"
          value={length}
          onChange={(e) => onLength(+e.target.value)}
        >
          <option value={0}>Grid</option>
          {lengths.map((n) => (
            <option key={n} value={n}>
              {n} ticks
            </option>
          ))}
        </select>
      </label>
      <button
        onClick={fit}
        title="Fit notes — restore pitch/time view for selection or part · Shift+5"
        aria-label="Fit notes"
      >
        Fit notes
      </button>
      <label>
        Rows{" "}
        <input
          aria-label="Pitch row height"
          type="range"
          min="10"
          max="30"
          value={row}
          onChange={(e) => onRow(+e.target.value)}
        />
      </label>
      <label
        title="Show all notes of the current source in the pitch view"
      >
        <input
          type="checkbox"
          checked={ghosts}
          onChange={(e) => onGhosts(e.target.checked)}
        />{" "}
        Ghosts
      </label>
      <label>
        <input
          type="checkbox"
          checked={follow}
          onChange={(e) => onFollow(e.target.checked)}
        />{" "}
        Follow playback
      </label>
      <p className="muted piano-tool-hint">
        Smart Tool (Select): empty click adds a note; drag empty space
        marquee-selects; drag notes to move; edge-drag resizes; right-drag
        erases. Draw (P) · Paint (B) · Erase (D) · Cursor (2) · Pan (3) ·
        middle-drag pans. Hold-right 1s opens note properties.
      </p>
      <details className="piano-tool-menu">
        <summary>Tools</summary>
        <div>
          <select
            aria-label="Note transform"
            value={operation}
            onChange={(e) => {
              const op = e.target.value as Transform;
              setOperation(op);
              setAmount(
                op === "strum" || op === "flam" || op === "drumRoll"
                  ? Math.max(1, Math.round(view.ppq / 32))
                  : op === "beatRepeat"
                    ? Math.max(1, grid)
                    : op === "fitToBars"
                      ? 1
                      : op === "velocityRandom"
                        ? 10
                        : 100,
              );
            }}
          >
            {operations.map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
          {operation === "quantize" ? (
            <>
              <label>
                Target{" "}
                <select
                  aria-label="Quantize target"
                  value={quantizeTarget}
                  onChange={(e) =>
                    setQuantizeTarget(e.target.value as QuantizeTarget)
                  }
                >
                  <option value="starts">Note starts</option>
                  <option value="ends">Note ends</option>
                  <option value="both">Starts and ends</option>
                </select>
              </label>
              <label>
                Strength %{" "}
                <input
                  aria-label="Quantize strength"
                  type="number"
                  min="0"
                  max="100"
                  value={strength}
                  onChange={(e) => setStrength(clamp(+e.target.value, 0, 100))}
                />
              </label>
              <label>
                Swing %{" "}
                <input
                  aria-label="Quantize swing"
                  type="number"
                  min="0"
                  max="100"
                  value={swing}
                  onChange={(e) => setSwing(clamp(+e.target.value, 0, 100))}
                />
              </label>
              <label>
                Tuplet{" "}
                <select
                  aria-label="Quantize tuplet"
                  value={tuplet}
                  onChange={(e) => setTuplet(+e.target.value)}
                >
                  <option value={0}>Off</option>
                  <option value={3}>3</option>
                  <option value={5}>5</option>
                  <option value={7}>7</option>
                </select>
              </label>
              <label title="Only move notes within this many ticks of a grid point (0 = all)">
                Catch ticks{" "}
                <input
                  aria-label="Quantize catch range"
                  type="number"
                  min="0"
                  max="960"
                  value={catchRange}
                  onChange={(e) =>
                    setCatchRange(clamp(+e.target.value, 0, 960))
                  }
                />
              </label>
              <label title="Skip notes already within this many ticks of the grid">
                Safe ticks{" "}
                <input
                  aria-label="Quantize safe range"
                  type="number"
                  min="0"
                  max="960"
                  value={safeRange}
                  onChange={(e) =>
                    setSafeRange(clamp(+e.target.value, 0, 960))
                  }
                />
              </label>
              <label title="Add seeded ±jitter after snap (0 = exact)">
                Rough ticks{" "}
                <input
                  aria-label="Quantize rough timing"
                  type="number"
                  min="0"
                  max="240"
                  value={roughTicks}
                  onChange={(e) =>
                    setRoughTicks(clamp(+e.target.value, 0, 240))
                  }
                />
              </label>
              {quantizeTarget === "starts" ? (
                <label>
                  <input
                    type="checkbox"
                    checked={duration}
                    onChange={(e) => setDuration(e.target.checked)}
                  />{" "}
                  Quantize lengths
                </label>
              ) : null}
              <label
                title="Move CC and pitch-bend timing with changed note starts (SFM Move controllers)"
              >
                <input
                  type="checkbox"
                  checked={moveControllers}
                  onChange={(e) => setMoveControllers(e.target.checked)}
                  aria-label="Move controllers with note starts"
                />{" "}
                Move controllers
              </label>
            </>
          ) : ![
              "legato",
              "chop",
              "glue",
              "flipTime",
              "flipPitch",
              "limit",
            ].includes(operation) ? (
            <label>
              {operation === "fitToBars"
                ? "Bars "
                : operation === "beatRepeat"
                  ? "Period ticks "
                  : "Amount "}
              <input
                aria-label="Transform amount"
                type="number"
                min="0"
                max="10000"
                value={amount}
                onChange={(e) => setAmount(clamp(+e.target.value, 0, 10000))}
              />
            </label>
          ) : null}
          {operation === "limit" && (
            <>
              <label>
                Low pitch{" "}
                <input
                  aria-label="Limit low pitch"
                  type="number"
                  min="0"
                  max={high}
                  value={low}
                  onChange={(e) => setLow(clamp(+e.target.value, 0, high))}
                />
              </label>
              <label>
                High pitch{" "}
                <input
                  aria-label="Limit high pitch"
                  type="number"
                  min={low}
                  max="127"
                  value={high}
                  onChange={(e) => setHigh(clamp(+e.target.value, low, 127))}
                />
              </label>
            </>
          )}
          <button disabled={disabled || !chosen.length} onClick={apply}>
            Apply to selection
          </button>
          <button
            disabled={disabled || !notes.length}
            title="Delete every note on this source in the current section (one undo)"
            onClick={() => {
              if (
                !window.confirm(
                  `Clear Part: delete all ${notes.length} note(s) on CH${channel} in ${section.label}? Controllers, Voice, and CASM stay.`,
                )
              )
                return;
              void edit({
                type: "deleteNotes",
                ids: notes.map((n) => n.id),
              });
            }}
          >
            Clear Part
          </button>
          <small>{advisory.text}</small>
          <small>Each tool commits once. CASM is preserved.</small>
        </div>
      </details>
      {onNoteColorsChange ? (
        <details className="piano-tool-menu note-colors-menu">
          <summary>Note colors</summary>
          <div className="note-colors-grid" role="group" aria-label="Note colors">
            {colors.map((hex, i) => (
              <label key={i} title={`CH${i + 1} display color`}>
                <span>CH{i + 1}</span>
                <input
                  type="color"
                  aria-label={`Note color CH${i + 1}`}
                  value={hex}
                  disabled={disabled}
                  onChange={(e) => {
                    const next = [...colors];
                    next[i] = e.target.value;
                    onNoteColorsChange(sanitizeNoteColors(next));
                  }}
                />
              </label>
            ))}
            <button
              type="button"
              disabled={disabled}
              onClick={() => onNoteColorsChange([...DEFAULT_NOTE_COLORS])}
            >
              Reset colors
            </button>
            <small>Display only — does not change MIDI.</small>
          </div>
        </details>
      ) : null}
      <span className="piano-hint" title={advisory.text}>
        {advisory.text}
      </span>
    </div>
  );
}
