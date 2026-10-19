/**
 * Section Auto Pan → CC10 points (SFM-MIXER-AUTOPAN).
 * Independently authored generator: sine / triangle / square LFO over the
 * section length. Pan values stay in 1..127 (XG pan 0 can mean Random on drums).
 */

export type AutoPanShape = "sine" | "triangle" | "square";

export type AutoPanOptions = {
  lengthTicks: number;
  ppq: number;
  /** Cycle length in beats (quarter notes at this PPQ). */
  beats: number;
  shape: AutoPanShape;
  /** 0..100 percent depth. */
  depth: number;
  reverse?: boolean;
  /** Absolute section start tick. */
  sectionStart: number;
};

export type AutoPanPoint = { tick: number; value: number };

function clampByte(n: number): number {
  return Math.max(0, Math.min(127, Math.round(n)));
}

/** Generate absolute-tick CC10 points for one section. */
export function autoPanPoints(opts: AutoPanOptions): AutoPanPoint[] {
  const {
    lengthTicks,
    ppq,
    beats,
    shape,
    depth,
    reverse = false,
    sectionStart,
  } = opts;
  if (!(lengthTicks > 0 && ppq > 0 && beats > 0)) {
    throw new Error("Invalid Auto Pan timing");
  }
  const period = ppq * beats;
  const step = Math.max(
    1,
    Math.ceil(lengthTicks / 12000),
    Math.round(Math.min(ppq / 24, period / 24)),
  );
  const amount = Math.max(0, Math.min(100, depth)) / 100;
  const points: AutoPanPoint[] = [];
  let previous = -1;
  const stride = shape === "square" ? Math.max(1, Math.round(period / 2)) : step;
  for (let tick = 0; tick < lengthTicks; tick += stride) {
    const phase = (tick % period) / period;
    let wave =
      shape === "square"
        ? phase < 0.5
          ? -1
          : 1
        : shape === "triangle"
          ? 1 - 4 * Math.abs(phase - 0.5)
          : -Math.cos(phase * 2 * Math.PI);
    if (reverse) wave *= -1;
    const value = clampByte(64 + wave * 63 * amount);
    // Keep drum-safe pan away from 0 (XG Random).
    const safe = value === 0 ? 1 : value;
    if (safe !== previous) {
      points.push({ tick: sectionStart + tick, value: safe });
      previous = safe;
    }
  }
  return points;
}
