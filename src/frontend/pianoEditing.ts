import type {
  Command,
  Note,
  NoteInput,
  NoteUpdate,
  Section,
  View,
} from "./contracts";

export const clamp = (n: number, lo: number, hi: number) =>
  Math.max(lo, Math.min(hi, n));
export const tickSnap = (tick: number, step: number) =>
  Math.round(Math.round(tick / Math.max(1, step)) * Math.max(1, step));
/** Effective grid step for optional tuplet subdivision (3/5/7). */
export const quantizeStep = (step: number, tuplet?: number) => {
  const base = Math.max(1, step);
  const t = tuplet && [3, 5, 7].includes(tuplet) ? tuplet : 0;
  return t ? Math.max(1, Math.round((base * 4) / t)) : base;
};
/** Swung grid candidate: odd indices delayed by step×swing/300 (standard swing). */
export const swungGridTick = (index: number, step: number, swing = 0) => {
  const s = Math.max(1, step);
  const delay =
    index % 2 === 1
      ? Math.round((s * clamp(swing, 0, 100)) / 300)
      : 0;
  return index * s + delay;
};
/** Nearest swung (or plain) grid tick relative to section start. */
export const nearestGridTick = (
  rel: number,
  step: number,
  swing = 0,
): number => {
  const s = Math.max(1, step);
  if (!swing) return tickSnap(rel, s);
  const approx = Math.round(rel / s);
  let best = swungGridTick(approx, s, swing);
  for (let i = approx - 3; i <= approx + 3; i++) {
    if (i < 0) continue;
    const t = swungGridTick(i, s, swing);
    if (Math.abs(t - rel) < Math.abs(best - rel)) best = t;
  }
  return best;
};
export type QuantizeTarget = "starts" | "ends" | "both";
export const noteUpdate = (n: Note): NoteUpdate => ({
  id: n.id,
  start: n.start,
  duration: n.duration,
  pitch: n.pitch,
  velocity: n.velocity,
});
export const noteInput = (n: Note): NoteInput => ({
  track: n.track,
  channel: n.channel,
  start: n.start,
  duration: n.duration,
  pitch: n.pitch,
  velocity: n.velocity,
});
/** A swept erase stroke, rather than its potentially much larger bounding box. */
export function segmentHitsRect(
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  r: { x: number; y: number; w: number; h: number },
) {
  let first = 0,
    last = 1;
  for (const [origin, delta, low, high] of [
    [x0, x1 - x0, r.x, r.x + r.w],
    [y0, y1 - y0, r.y, r.y + r.h],
  ]) {
    if (delta === 0) {
      if (origin < low || origin > high) return false;
      continue;
    }
    const a = (low - origin) / delta,
      b = (high - origin) / delta;
    first = Math.max(first, Math.min(a, b));
    last = Math.min(last, Math.max(a, b));
    if (first > last) return false;
  }
  return true;
}
export function scopedNotes(view: View, section: Section, channel: number) {
  return view.notes.filter(
    (n) =>
      n.channel === channel &&
      n.start >= section.start &&
      n.start < section.end,
  );
}
/** Read the retained meter at a section instead of the header-only View meter. */
export function meterAt(view: View, tick: number) {
  let numerator = 4,
    denominator = 4;
  for (const e of [...view.events]
    .filter((e) => e.tick <= tick)
    .sort((a, b) => a.tick - b.tick || a.order - b.order)) {
    if (
      e.data[0] === 255 &&
      e.data[1] === 0x58 &&
      e.data[2] === 4 &&
      e.data[3] > 0 &&
      e.data[4] < 8
    ) {
      numerator = e.data[3];
      denominator = 2 ** e.data[4];
    }
  }
  // Fixtures/views without raw metadata still have the established header fields.
  if (
    !view.events.some(
      (e) => e.data[0] === 255 && e.data[1] === 0x58 && e.tick <= tick,
    )
  ) {
    numerator = view.numerator;
    denominator = view.denominator;
  }
  return {
    numerator,
    denominator,
    beat: (view.ppq * 4) / denominator,
    bar: ((view.ppq * 4) / denominator) * numerator,
  };
}
export function tickFromElapsed(
  view: View,
  section: Section,
  elapsedMs: number,
) {
  const tempos = view.events
    .filter((e) => e.data[0] === 255 && e.data[1] === 0x51 && e.data[2] === 3)
    .sort((a, b) => a.tick - b.tick || a.order - b.order);
  const tempoValue = (data: number[]) =>
    data[3] * 65536 + data[4] * 256 + data[5];
  let tempo = 60_000_000 / view.tempo,
    tick = section.start,
    remaining = Math.max(0, elapsedMs);
  for (const e of tempos)
    if (e.tick <= section.start && tempoValue(e.data) > 0)
      tempo = tempoValue(e.data);
  for (const e of tempos.filter(
    (e) => e.tick > section.start && e.tick < section.end,
  )) {
    const ms = ((e.tick - tick) * tempo) / view.ppq / 1000;
    if (remaining < ms) return tick + (remaining * 1000 * view.ppq) / tempo;
    remaining -= ms;
    tick = e.tick;
    if (tempoValue(e.data) > 0) tempo = tempoValue(e.data);
  }
  return Math.min(section.end, tick + (remaining * 1000 * view.ppq) / tempo);
}
export function moveBounds(
  notes: Note[],
  section: Section,
  dt: number,
  dp: number,
) {
  if (!notes.length) return { dt: 0, dp: 0 };
  return {
    dt: clamp(
      dt,
      Math.max(...notes.map((n) => section.start - n.start)),
      Math.min(...notes.map((n) => section.end - n.start - n.duration)),
    ),
    dp:
      clamp(
        dp,
        -Math.min(...notes.map((n) => n.pitch)),
        127 - Math.max(...notes.map((n) => n.pitch)),
      ) || 0,
  };
}
export function validateDraft(
  notes: (NoteInput | NoteUpdate)[],
  section: Section,
) {
  if (
    notes.length > 50_000 ||
    notes.some(
      (n) =>
        ![n.start, n.duration, n.pitch, n.velocity].every(
          Number.isSafeInteger,
        ) ||
        n.start < section.start ||
        n.start >= section.end ||
        n.duration < 1 ||
        n.start + n.duration > section.end ||
        n.pitch < 0 ||
        n.pitch > 127 ||
        n.velocity < 1 ||
        n.velocity > 127,
    )
  ) {
    throw new Error(
      "Operation exceeds MIDI or section bounds. Adjust the selection or tool settings.",
    );
  }
}
export type Transform =
  | "quantize"
  | "legato"
  | "gate"
  | "chop"
  | "glue"
  | "strum"
  | "flam"
  | "drumRoll"
  | "flipTime"
  | "flipPitch"
  | "limit"
  | "stretch"
  | "fitToBars"
  | "beatRepeat"
  | "velocityScale"
  | "velocityRandom";
export function transformNotes(
  kind: Transform,
  notes: Note[],
  section: Section,
  channel: number,
  options: {
    step: number;
    strength?: number;
    amount?: number;
    quantizeDuration?: boolean;
    moveControllers?: boolean;
    seed?: number;
    low?: number;
    high?: number;
    /** Ticks per bar for fitToBars (from section meter). */
    barTicks?: number;
    /** Swing % delaying odd grid slots (0–100). */
    swing?: number;
    /** Tuplet subdivision 3/5/7 (0 = off). */
    tuplet?: number;
    /** Which endpoints to snap. */
    quantizeTarget?: QuantizeTarget;
    /** Only catch (move) notes within this many ticks of a grid point (0 = all). */
    catchRange?: number;
    /** Skip notes already within this many ticks of the grid. */
    safeRange?: number;
    /** After snap, add seeded ±roughTicks jitter. */
    roughTicks?: number;
  },
): Command {
  if (!notes.length) throw new Error("Select notes first.");
  const step = Math.max(1, options.step),
    amount = options.amount ?? 100;
  let updates = notes.map(noteUpdate),
    add: NoteInput[] = [],
    remove: number[] = [];
  const first = Math.min(...notes.map((n) => n.start)),
    last = Math.max(...notes.map((n) => n.start + n.duration));
  if (kind === "quantize") {
    const strength = clamp(options.strength ?? 100, 0, 100) / 100;
    const swing = clamp(options.swing ?? 0, 0, 100);
    const qStep = quantizeStep(step, options.tuplet);
    const targetMode: QuantizeTarget = options.quantizeTarget ?? "starts";
    const catchRange = Math.max(0, Math.round(options.catchRange ?? 0));
    const safeRange = Math.max(0, Math.round(options.safeRange ?? 0));
    const rough = Math.max(0, Math.round(options.roughTicks ?? 0));
    let seed = (options.seed ?? 1) >>> 0;
    const rand = () => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      return seed / 0x100000000;
    };
    const snapRel = (rel: number) => nearestGridTick(rel, qStep, swing);
    const withinCatch = (rel: number, snapped: number) =>
      !catchRange || Math.abs(rel - snapped) <= catchRange;
    const outsideSafe = (rel: number, snapped: number) =>
      !safeRange || Math.abs(rel - snapped) > safeRange;
    const snapStarts = targetMode === "starts" || targetMode === "both";
    const snapEnds = targetMode === "ends" || targetMode === "both";
    const snapLengths =
      !!options.quantizeDuration && targetMode === "starts";
    updates = updates.map((n) => {
      let start = n.start;
      let duration = n.duration;
      if (snapStarts) {
        const rel = n.start - section.start;
        const snapped = snapRel(rel);
        if (withinCatch(rel, snapped) && outsideSafe(rel, snapped)) {
          start = clamp(
            Math.round(
              n.start + (section.start + snapped - n.start) * strength,
            ),
            section.start,
            section.end - 1,
          );
        }
      }
      if (snapEnds) {
        const end = n.start + n.duration;
        const relEnd = end - section.start;
        const snappedEnd = snapRel(relEnd);
        if (withinCatch(relEnd, snappedEnd) && outsideSafe(relEnd, snappedEnd)) {
          const newEnd = Math.round(
            end + (section.start + snappedEnd - end) * strength,
          );
          duration = Math.max(1, newEnd - start);
        }
      } else if (snapLengths) {
        duration = Math.max(
          1,
          Math.round(
            n.duration +
              (Math.max(1, tickSnap(n.duration, qStep)) - n.duration) *
                strength,
          ),
        );
      }
      if (rough > 0) {
        const absoluteEnd = start + duration;
        const j = Math.round((rand() * 2 - 1) * rough);
        start = clamp(start + j, section.start, section.end - 1);
        if (snapEnds) duration = Math.max(1, absoluteEnd - start);
      }
      return {
        ...n,
        start,
        duration: Math.min(Math.max(1, duration), section.end - start),
      };
    });
  }
  if (kind === "legato")
    updates = updates.map((n) => ({
      ...n,
      duration:
        Math.min(
          ...notes.filter((m) => m.start > n.start).map((m) => m.start),
          section.end,
        ) - n.start,
    }));
  if (kind === "gate")
    updates = updates.map((n) => ({
      ...n,
      duration: Math.max(1, Math.round((n.duration * amount) / 100)),
    }));
  if (kind === "stretch")
    updates = updates.map((n) => ({
      ...n,
      start: first + Math.round(((n.start - first) * amount) / 100),
      duration: Math.max(1, Math.round((n.duration * amount) / 100)),
    }));
  if (kind === "fitToBars") {
    const bar = Math.max(1, options.barTicks ?? step * 4);
    const bars = Math.max(1, Math.round(amount));
    const target = bars * bar;
    const span = Math.max(1, last - first);
    updates = updates.map((n) => {
      const start =
        first + Math.round(((n.start - first) * target) / span);
      const end =
        first +
        Math.round(((n.start + n.duration - first) * target) / span);
      return {
        ...n,
        start,
        duration: Math.max(1, end - start),
      };
    });
  }
  if (kind === "beatRepeat") {
    const period = Math.max(1, Math.round(amount) || step);
    const span = Math.max(period, last - first);
    add = [];
    for (let offset = span; first + offset < section.end; offset += span) {
      for (const n of notes) {
        const start = n.start + offset;
        if (start >= section.end) continue;
        add.push({
          ...noteInput(n),
          start,
          duration: Math.min(n.duration, section.end - start),
        });
        if (add.length > 50_000)
          throw new Error("Beat Repeat would create more than 50,000 notes.");
      }
    }
  }
  if (kind === "flipTime")
    updates = updates.map((n) => ({
      ...n,
      start: first + last - n.start - n.duration,
    }));
  if (kind === "flipPitch") {
    const sum =
      Math.min(...notes.map((n) => n.pitch)) +
      Math.max(...notes.map((n) => n.pitch));
    updates = updates.map((n) => ({ ...n, pitch: sum - n.pitch }));
  }
  if (kind === "limit")
    updates = updates.map((n) => ({
      ...n,
      pitch: clamp(n.pitch, options.low ?? 0, options.high ?? 127),
    }));
  if (kind === "strum") {
    const groups = new Map<number, Note[]>();
    for (const n of notes)
      groups.set(n.start, [...(groups.get(n.start) ?? []), n]);
    const shifts = new Map<number, number>();
    for (const group of groups.values())
      group
        .sort((a, b) => a.pitch - b.pitch || a.id - b.id)
        .forEach((n, i) => shifts.set(n.id, Math.round(i * amount)));
    updates = updates.map((n) => ({
      ...n,
      start: n.start + shifts.get(n.id)!,
    }));
  }
  if (kind === "flam")
    add = notes.map((n) => ({
      ...noteInput(n),
      start: n.start - Math.max(1, Math.round(amount)),
      duration: Math.max(1, Math.round(amount)),
      velocity: Math.max(1, Math.round(n.velocity * 0.65)),
    }));
  if (kind === "drumRoll") {
    // amount = spacing ticks between hits; strength (0..100) unused → fixed 4 lead-in hits.
    const spacing = Math.max(1, Math.round(amount));
    const hits = 4;
    add = [];
    for (const n of notes) {
      for (let i = hits; i >= 1; i--) {
        const start = n.start - i * spacing;
        if (start < section.start) continue;
        add.push({
          ...noteInput(n),
          start,
          duration: Math.min(n.duration, spacing),
          velocity: Math.max(1, Math.round(n.velocity * (0.35 + 0.12 * (hits - i)))),
        });
        if (add.length > 50_000)
          throw new Error("Drum Roll would create more than 50,000 notes.");
      }
    }
  }
  if (kind === "velocityScale")
    updates = updates.map((n) => ({
      ...n,
      velocity: clamp(Math.round((n.velocity * amount) / 100), 1, 127),
    }));
  if (kind === "velocityRandom") {
    let seed = (options.seed ?? 12345) >>> 0;
    updates = updates.map((n) => {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      return {
        ...n,
        velocity: clamp(
          n.velocity + Math.round(((seed / 0xffffffff) * 2 - 1) * amount),
          1,
          127,
        ),
      };
    });
  }
  if (kind === "chop") {
    updates = [];
    remove = notes.map((n) => n.id);
    for (const n of notes) {
      const count = Math.ceil(n.duration / step);
      if (count + add.length > 50_000)
        throw new Error("Chop would create more than 50,000 notes.");
      for (let i = 0; i < count; i++) {
        const start = n.start + Math.round(i * step),
          end = Math.min(
            n.start + n.duration,
            n.start + Math.round((i + 1) * step),
          );
        if (end > start)
          add.push({ ...noteInput(n), start, duration: end - start });
      }
    }
  }
  if (kind === "glue") {
    updates = [];
    const sorted = [...notes].sort(
      (a, b) =>
        a.track - b.track ||
        a.pitch - b.pitch ||
        a.start - b.start ||
        a.id - b.id,
    );
    for (let i = 0; i < sorted.length;) {
      const n = sorted[i++];
      let end = n.start + n.duration;
      while (
        i < sorted.length &&
        sorted[i].track === n.track &&
        sorted[i].pitch === n.pitch &&
        sorted[i].start <= end
      ) {
        end = Math.max(end, sorted[i].start + sorted[i].duration);
        remove.push(sorted[i++].id);
      }
      updates.push({ ...noteUpdate(n), duration: end - n.start });
    }
  }
  validateDraft([...updates, ...add], section);
  return {
    type: "pianoNotes",
    section: section.id,
    channel,
    updates,
    add,
    remove,
    moveControllers: options.moveControllers !== false,
  };
}

/** SFM V2.0.44 lane set: 1,7,10,11,64,71,72,73,74,91,93 (+ velocity / EQ / pitch separate). */
export const controllerLanes = [
  [7, "Volume · CC7"],
  [11, "Expression · CC11"],
  [1, "Modulation · CC1"],
  [10, "Pan · CC10"],
  [64, "Sustain · CC64"],
  [71, "Resonance · CC71"],
  [72, "Release · CC72"],
  [73, "Attack · CC73"],
  [74, "Cutoff · CC74"],
  [91, "Reverb send · CC91"],
  [93, "Chorus send · CC93"],
] as const;

/** Virtual lane ids for timed EQ High/Low NRPN automation (not MIDI CC). */
export const EQ_LOW_LANE = -2;
export const EQ_HIGH_LANE = -3;
/** Virtual lane id for 14-bit pitch bend automation. */
export const PITCH_BEND_LANE = -4;
/** Virtual lane id for section tempo automation (SMF meta FF 51). */
export const TEMPO_LANE = -5;

/** BPM from SMF tempo meta payload `[FF 51 03 tt tt tt]`. */
export function tempoBpmFromMeta(data: number[]): number | null {
  if (data.length < 6 || data[0] !== 0xff || data[1] !== 0x51 || data[2] !== 3)
    return null;
  const us = ((data[3] & 0xff) << 16) | ((data[4] & 0xff) << 8) | (data[5] & 0xff);
  if (us <= 0) return null;
  return 60_000_000 / us;
}

export function eqBandFromLane(
  lane: number,
): "eqLow" | "eqHigh" | null {
  if (lane === EQ_LOW_LANE) return "eqLow";
  if (lane === EQ_HIGH_LANE) return "eqHigh";
  return null;
}

export function bend14(data: number[]): number {
  if (data.length < 3) return 8192;
  return (data[1] & 127) | ((data[2] & 127) << 7);
}

export function bendBytes(value: number, channel: number): number[] {
  const v = clamp(Math.round(value), 0, 16383);
  return [0xe0 | (channel - 1), v & 127, (v >> 7) & 127];
}

/**
 * Collapse EQ NRPN packets in a section into drawable points (one per tick).
 * Point `id` is the Data Entry (CC6) event id for selection/erase.
 */
export function eqLanePoints(
  events: { id: number; tick: number; order: number; data: number[] }[],
  sectionStart: number,
  sectionEnd: number,
  channel: number,
  band: "eqLow" | "eqHigh",
): { id: number; tick: number; order: number; data: number[] }[] {
  if (!(channel >= 1 && channel <= 16)) return [];
  const status = 0xb0 | (channel - 1);
  const wantLsb = band === "eqLow" ? 48 : 49;
  const sorted = events
    .filter(
      (e) =>
        e.tick >= sectionStart &&
        e.tick < sectionEnd &&
        e.data.length === 3 &&
        e.data[0] === status,
    )
    .sort((a, b) => a.tick - b.tick || a.order - b.order || a.id - b.id);
  const out: { id: number; tick: number; order: number; data: number[] }[] =
    [];
  let msb: number | null = null;
  let lsb: number | null = null;
  let tick = -1;
  for (const e of sorted) {
    if (e.tick !== tick) {
      msb = null;
      lsb = null;
      tick = e.tick;
    }
    const [cc, v] = [e.data[1], e.data[2]];
    if (cc === 99) {
      msb = v;
      lsb = null;
    } else if (cc === 98) {
      lsb = v;
    } else if (cc === 6) {
      if (msb === 1 && lsb === wantLsb) {
        const existing = out.findIndex((p) => p.tick === e.tick);
        const point = {
          id: e.id,
          tick: e.tick,
          order: e.order,
          data: [status, 6, v],
        };
        if (existing >= 0) out[existing] = point;
        else out.push(point);
      }
      msb = null;
      lsb = null;
    } else if (cc === 100 || cc === 101) {
      msb = null;
      lsb = null;
    }
  }
  return out;
}

/** Last EQ Data Entry value with tick strictly before `beforeTick`, or null. */
export function eqValueBefore(
  events: { id: number; tick: number; order: number; data: number[] }[],
  beforeTick: number,
  channel: number,
  band: "eqLow" | "eqHigh",
): number | null {
  const pts = eqLanePoints(events, 0, beforeTick, channel, band);
  return pts.at(-1)?.data[2] ?? null;
}

/** Source-pattern advisory only. CASM output limits are not source-note constraints. */
export function casmAdvisory(
  view: View,
  section: Section,
  channel: number,
  notes: Note[],
) {
  const routes = view.routes.filter(
    (r) => r.source === channel && r.sections.includes(section.label),
  );
  const drum =
    routes.some((r) => r.destination === 9 || r.destination === 10) ||
    (!routes.length && (channel === 9 || channel === 10));
  const outside = routes.some(
    (r) =>
      r.bytes.length === 47 &&
      notes.some((n) => {
        const o = n.pitch < r.bytes[20] ? 22 : n.pitch > r.bytes[21] ? 34 : 28;
        return n.pitch < r.bytes[o + 3] || n.pitch > r.bytes[o + 4];
      }),
  );
  return {
    drum,
    text: drum
      ? "Rhythm destination · pitch edits change drum assignments."
      : outside
        ? "Selection extends beyond a CASM output range. Review Source tracks and Chord preview."
        : routes.length
          ? "CASM source rules · preview chords after pitch edits"
          : "No decoded CASM route · source MIDI editing",
  };
}
