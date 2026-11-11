import { useState } from "react";
import { HintPanel } from "../kit/hint";
import { Knob } from "../kit/Knob";
import peakRef from "./ref/peak-knobs.png";

/** Dev-only: every kit control next to its FL reference crop at the same scale. */
export function Gallery() {
  const [v, setV] = useState({ base: 0, vol: 0.8, decay: 0.7, speed: 0.55 });
  const set = (k: keyof typeof v) => (n: number) => setV((p) => ({ ...p, [k]: n }));
  const S = 4;
  return (
    <div style={{ padding: 16, background: "var(--fl-body)", minHeight: "100vh" }}>
      <HintPanel context="Gallery" />
      <h3 style={{ color: "var(--fl-caption-text)", fontWeight: 400 }}>Knob: FL (top) vs ours (bottom), ×{S}</h3>
      <img
        src={peakRef}
        width={197 * S}
        height={88 * S}
        alt="FL Peak Controller knobs"
        style={{ imageRendering: "pixelated", display: "block" }}
      />
      <div style={{ zoom: S, display: "flex", gap: 10, padding: "12px 10px", width: 197, boxSizing: "border-box" }}>
        <Knob label="Base" value={v.base} onChange={set("base")} />
        <Knob label="Vol" value={v.vol} bipolar defaultValue={0.5} onChange={set("vol")} />
        <Knob label="Tension" value={v.speed} onChange={set("speed")} color="var(--fl-green)" />
        <Knob label="Decay" value={v.decay} onChange={set("decay")} />
      </div>
    </div>
  );
}
