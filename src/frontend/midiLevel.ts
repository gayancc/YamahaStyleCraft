/** SFM MIDI LEVEL — estimated Style playback activity (not audio dB). */

export type MidiLevelBand = "safe" | "warn" | "hot";

/** Velocity × Volume × Expression estimate at playhead for audible channels (0..1). */
export function estimateMidiLevel(
  notes: {
    channel: number;
    start: number;
    duration: number;
    velocity: number;
  }[],
  playheadTick: number | null,
  audibleChannels: number[],
  volumeAt: (channel: number) => number,
  /** CC11 Expression; defaults to 127 (full) when omitted. */
  expressionAt?: (channel: number) => number,
): number {
  if (playheadTick == null || !audibleChannels.length) return 0;
  let master = 0;
  for (const ch of audibleChannels) {
    const vol = Math.max(0, Math.min(127, volumeAt(ch))) / 127;
    const expr =
      Math.max(0, Math.min(127, expressionAt?.(ch) ?? 127)) / 127;
    let part = 0;
    for (const n of notes) {
      if (n.channel !== ch) continue;
      if (n.start <= playheadTick && playheadTick < n.start + n.duration) {
        part = Math.max(part, (n.velocity / 127) * vol * expr);
      }
    }
    master = Math.max(master, part);
  }
  return master;
}

/** SFM band thresholds for LED zones. */
export function midiLevelBand(level: number): MidiLevelBand {
  if (level >= 0.88) return "hot";
  if (level >= 0.72) return "warn";
  return "safe";
}

/** Peak marker hold (~0.7s in SFM help). */
export function holdMidiPeak(
  prevPeak: number,
  peakAtMs: number,
  level: number,
  nowMs: number,
  holdMs = 700,
): { peak: number; peakAtMs: number } {
  if (level >= prevPeak - 1e-6) return { peak: level, peakAtMs: nowMs };
  if (nowMs - peakAtMs >= holdMs) return { peak: level, peakAtMs: nowMs };
  return { peak: prevPeak, peakAtMs };
}

export const MIDI_LEVEL_SEGMENTS = 20;

export function midiLevelSegments(
  level: number,
  peak: number,
  count = MIDI_LEVEL_SEGMENTS,
): { zone: MidiLevelBand; lit: boolean; peak: boolean }[] {
  const v = Math.max(0, Math.min(1, level));
  const p = Math.max(0, Math.min(1, peak));
  const litCount = v > 0.001 ? Math.ceil(v * count) : 0;
  const peakIndex =
    p > 0.001 ? Math.min(count - 1, Math.ceil(p * count) - 1) : -1;
  return Array.from({ length: count }, (_, i) => {
    const position = (i + 0.5) / count;
    return {
      zone: midiLevelBand(position),
      lit: i < litCount,
      peak: i === peakIndex,
    };
  });
}
