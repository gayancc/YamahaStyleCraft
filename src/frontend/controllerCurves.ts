import { clamp } from "./pianoEditing";

/** Map 0..1 progress through a controller gesture shape (SFM-064). */
export function curveT(
  mode: "draw" | "ramp" | "easeIn" | "easeOut" | "step" | "relative" | string,
  t: number,
): number {
  const x = clamp(t, 0, 1);
  if (mode === "easeIn") return x * x;
  if (mode === "easeOut") return 1 - (1 - x) * (1 - x);
  if (mode === "step") return x < 1 ? 0 : 1;
  return x;
}

export const isSpanCurve = (mode: string) =>
  mode === "ramp" || mode === "easeIn" || mode === "easeOut" || mode === "step";
