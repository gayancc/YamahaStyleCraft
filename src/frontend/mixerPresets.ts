/**
 * Yamaha Safe FX mixer presets (SFM-MIXER-SAFE-FX).
 * Black-box observed from live SFM V2.0.44: applies only MIDI CC values
 * (reverb/chorus/cutoff/resonance) — never proprietary DSP SysEx.
 */

export type SafeFxKey = "dry" | "tight" | "warm" | "wide" | "bright";

export type SafeFxPreset = {
  key: SafeFxKey;
  label: string;
  /** MIDI CC → value at section start. */
  values: { controller: number; value: number }[];
};

const cc = (controller: number, value: number) => ({ controller, value });

/** Independently authored from observed SFM preset → CC map. */
export const SAFE_FX_PRESETS: SafeFxPreset[] = [
  {
    key: "dry",
    label: "Dry / Reset FX",
    values: [cc(91, 0), cc(93, 0), cc(74, 64), cc(71, 64)],
  },
  {
    key: "tight",
    label: "Tight Rhythm",
    values: [cc(91, 18), cc(93, 0), cc(74, 68), cc(71, 60)],
  },
  {
    key: "warm",
    label: "Warm & Soft",
    values: [cc(91, 34), cc(93, 8), cc(74, 56), cc(71, 60)],
  },
  {
    key: "wide",
    label: "Wide Chorus",
    values: [cc(91, 42), cc(93, 44), cc(74, 64), cc(71, 64)],
  },
  {
    key: "bright",
    label: "Bright & Clear",
    values: [cc(91, 26), cc(93, 10), cc(74, 80), cc(71, 58)],
  },
];
