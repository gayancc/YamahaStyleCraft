/** Knob gesture maths. Pure, so the measured FL ratios are unit-testable.
 *
 * Evidence (docs/fl-ui-evidence/INTERACTION-EVIDENCE.md, LuxeVerb DECAY):
 *   drag 50 px            -> +1.583
 *   Ctrl + drag 50 px     -> +0.437   (ratio 0.276, i.e. fine divisor ~3.62)
 *   wheel, one notch      -> +0.426   (0.269 of a 50 px drag, i.e. ~13.45 px)
 * FL's parameter range was not recorded, so the absolute drag span (how many pixels sweep the whole range)
 * is NOT FL evidence. `DRAG_PX_FULL_RANGE` is an XP-defined default. */
export const DRAG_PX_FULL_RANGE = 208;
export const FINE_DIVISOR = 1 / 0.276;
export const WHEEL_NOTCH_PX = 13.45;

export const SWEEP_START_DEG = -135;
export const SWEEP_DEG = 270;

export const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

/** Normalised value after dragging from (startNorm, startY) to y. Up increases. */
export function dragNorm(startNorm: number, startY: number, y: number, fine: boolean): number {
  const px = startY - y;
  return clamp01(startNorm + px / DRAG_PX_FULL_RANGE / (fine ? FINE_DIVISOR : 1));
}

/** Normalised value after one wheel event. `deltaY` follows the DOM (negative = up = increase). */
export function wheelNorm(norm: number, deltaY: number, fine: boolean): number {
  const notches = -deltaY / 100;
  const px = notches * WHEEL_NOTCH_PX;
  return clamp01(norm + px / DRAG_PX_FULL_RANGE / (fine ? FINE_DIVISOR : 1));
}

/** Angle in degrees clockwise from 12 o'clock for a normalised value. */
export const angleOf = (norm: number): number => SWEEP_START_DEG + SWEEP_DEG * clamp01(norm);

/** Point on a circle at `deg` clockwise from 12 o'clock. */
export function polar(cx: number, cy: number, r: number, deg: number): { x: number; y: number } {
  const rad = ((deg - 90) * Math.PI) / 180;
  return { x: cx + r * Math.cos(rad), y: cy + r * Math.sin(rad) };
}

/** SVG arc path along a circle between two angles (degrees clockwise from 12 o'clock). */
export function arcPath(cx: number, cy: number, r: number, fromDeg: number, toDeg: number): string {
  const a = Math.min(fromDeg, toDeg);
  const b = Math.max(fromDeg, toDeg);
  if (b - a < 0.01) return "";
  const p0 = polar(cx, cy, r, a);
  const p1 = polar(cx, cy, r, b);
  const large = b - a > 180 ? 1 : 0;
  return `M ${p0.x.toFixed(2)} ${p0.y.toFixed(2)} A ${r} ${r} 0 ${large} 1 ${p1.x.toFixed(2)} ${p1.y.toFixed(2)}`;
}
