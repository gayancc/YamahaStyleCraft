import { clamp } from "./pianoEditing";
import { curveT } from "./controllerCurves";

/** Controllers offered by live SFM note-FX inspector (Genos-compatible CC). */
export const NOTE_FX_CONTROLLERS = [
  [1, "Modulation · CC1"],
  [11, "Expression · CC11"],
  [64, "Sustain · CC64"],
  [71, "Resonance · CC71"],
  [74, "Cutoff · CC74"],
  [91, "Reverb send · CC91"],
  [93, "Chorus send · CC93"],
] as const;

export const NOTE_FX_DEFAULTS: Record<number, number> = {
  1: 0,
  7: 100,
  10: 64,
  11: 127,
  64: 0,
  71: 64,
  72: 64,
  74: 64,
  91: 40,
  93: 0,
};

export type NoteFxCurve = "linear" | "ease-in" | "ease-out" | "step";

export type NoteFxSettings = {
  controller: number;
  startValue: number;
  endValue: number;
  curve: NoteFxCurve;
  restore: boolean;
};

export type NoteFxRange = {
  start: number;
  end: number;
  noteIds: number[];
};

const bound = (n: number, fallback = 0) =>
  clamp(Math.round(Number.isFinite(n) ? n : fallback), 0, 127);

const switchValue = (n: number) => (n >= 64 ? 127 : 0);

const curveMode = (curve: NoteFxCurve) => {
  if (curve === "ease-in") return "easeIn";
  if (curve === "ease-out") return "easeOut";
  if (curve === "step") return "step";
  return "ramp";
};

/** Merge overlapping selected notes into absolute tick ranges (end exclusive). */
export function mergeNoteFxRanges(
  notes: { id: number; start: number; duration: number }[],
  sectionEnd: number,
): NoteFxRange[] {
  const sorted = notes
    .map((n) => ({
      start: clamp(Math.round(n.start), 0, Math.max(0, sectionEnd - 1)),
      end: clamp(
        Math.round(n.start + Math.max(1, n.duration)),
        1,
        sectionEnd,
      ),
      noteIds: [n.id],
    }))
    .sort((a, b) => a.start - b.start || a.end - b.end);
  const ranges: NoteFxRange[] = [];
  for (const item of sorted) {
    const prev = ranges[ranges.length - 1];
    if (prev && item.start <= prev.end) {
      prev.end = Math.max(prev.end, item.end);
      prev.noteIds.push(...item.noteIds);
    } else ranges.push({ ...item, noteIds: [...item.noteIds] });
  }
  return ranges;
}

/** Last CC value for `controller` at or before `tick` (channel status already filtered). */
export function previousControllerValue(
  events: { tick: number; data: number[] }[],
  controller: number,
  tick: number,
): number {
  let value = NOTE_FX_DEFAULTS[controller] ?? 0;
  for (const e of [...events].sort((a, b) => a.tick - b.tick)) {
    if (e.tick > tick || e.data.length !== 3 || e.data[1] !== controller)
      continue;
    value = bound(e.data[2], value);
  }
  return value;
}

/**
 * Build absolute-tick CC points for one merged note range.
 * Matches live SFM density / sustain-binary / restore-at-end rules (plain MIDI CC only).
 */
export function buildNoteFxEnvelope(
  range: NoteFxRange,
  settings: NoteFxSettings,
  baselineEvents: { tick: number; data: number[] }[],
  ppq: number,
  sectionEnd: number,
): { tick: number; value: number }[] {
  const controller = settings.controller;
  const startValue =
    controller === 64
      ? switchValue(settings.startValue)
      : bound(settings.startValue);
  const endValue =
    controller === 64
      ? switchValue(settings.endValue)
      : bound(settings.endValue);
  const lastTick = Math.max(range.start, range.end - 1);
  // Do not inflate zero-width spans — points must stay on lastTick (inclusive).
  const duration = Math.max(0, lastTick - range.start);
  const pointCount =
    duration === 0
      ? 1
      : settings.curve === "step" || controller === 64 || startValue === endValue
        ? startValue === endValue
          ? 1
          : 2
        : Math.max(
            2,
            Math.min(16, Math.ceil(duration / Math.max(1, ppq / 8)) + 1),
          );
  const points: { tick: number; value: number }[] = [];
  const mode = curveMode(settings.curve);
  for (let i = 0; i < pointCount; i++) {
    const ratio = pointCount === 1 ? 0 : i / (pointCount - 1);
    const curved =
      settings.curve === "step" || controller === 64
        ? i === pointCount - 1
          ? 1
          : 0
        : curveT(mode, ratio);
    const tick =
      pointCount === 1
        ? lastTick
        : Math.round(range.start + duration * ratio);
    points.push({
      tick,
      value: bound(
        startValue + (endValue - startValue) * curved,
        pointCount === 1 ? endValue : startValue,
      ),
    });
  }
  if (settings.restore && range.end < sectionEnd) {
    points.push({
      tick: range.end,
      value: previousControllerValue(
        baselineEvents,
        controller,
        range.start,
      ),
    });
  }
  return points;
}

/**
 * CC event ids whose ticks fall in note spans.
 * Clear `[start, end)` always; include restore tick `end` only when `includeRestoreTick`.
 */
export function noteFxClearIds(
  events: { id: number; tick: number; data: number[] }[],
  channel: number,
  controller: number,
  ranges: NoteFxRange[],
  sectionEnd: number,
  includeRestoreTick = false,
): number[] {
  if (!(channel >= 1 && channel <= 16) || !ranges.length) return [];
  const status = 0xb0 | (channel - 1);
  const ids: number[] = [];
  for (const e of events) {
    if (e.data.length !== 3 || e.data[0] !== status || e.data[1] !== controller)
      continue;
    for (const r of ranges) {
      const hi =
        includeRestoreTick && r.end < sectionEnd
          ? Math.min(sectionEnd, r.end + 1)
          : r.end;
      if (e.tick >= r.start && e.tick < hi) {
        ids.push(e.id);
        break;
      }
    }
  }
  return ids;
}
