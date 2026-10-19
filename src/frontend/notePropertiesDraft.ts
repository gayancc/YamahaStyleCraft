import type { NoteUpdate, Section, View } from "./contracts";
import { meterAt, validateDraft } from "./pianoEditing";

const names = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
export const pitchName = (pitch: number) =>
  `${names[pitch % 12]}${Math.floor(pitch / 12) - 1}`;
export function formatNoteTime(tick: number, view: View, section: Section) {
  const meter = meterAt(view, section.start),
    relative = tick - section.start;
  const bar = Math.floor(relative / meter.bar),
    remainder = relative - bar * meter.bar;
  const beat = Math.floor(remainder / meter.beat);
  return `${bar + 1}:${beat + 1}:${Math.round(remainder - beat * meter.beat)}`;
}
export function formatNoteDuration(
  ticks: number,
  view: View,
  section: Section,
) {
  const meter = meterAt(view, section.start);
  const bars = Math.floor(ticks / meter.bar),
    remainder = ticks - bars * meter.bar;
  const beats = Math.floor(remainder / meter.beat);
  return `${bars}:${beats}:${Math.round(remainder - beats * meter.beat)}`;
}
export function parseNoteDuration(text: string, view: View, section: Section) {
  const parts = /^(\d+):(\d+):(\d+)$/.exec(text.trim()),
    meter = meterAt(view, section.start);
  if (!parts)
    throw Error("Enter length as bars:beats:ticks, starting at 0:0:0.");
  const [bars, beats, ticks] = parts.slice(1).map(Number);
  const duration = bars * meter.bar + beats * meter.beat + ticks;
  if (
    ![bars, beats, ticks, duration].every(Number.isSafeInteger) ||
    beats >= meter.numerator ||
    ticks >= meter.beat ||
    duration < 1
  )
    throw Error(
      "Length must be positive, with beats and ticks inside the current meter.",
    );
  return duration;
}
export type NotePropertiesDraft = {
  time: string;
  pitch: string;
  velocity: string;
  duration: string;
};
export function validateNoteProperties(
  draft: NotePropertiesDraft,
  id: number,
  view: View,
  section: Section,
): NoteUpdate {
  const parts = /^(\d+):(\d+):(\d+)$/.exec(draft.time.trim());
  if (!parts) throw Error("Enter time as bar:beat:tick, starting at 1:1:0.");
  const [bar, beat, tick] = parts.slice(1).map(Number),
    meter = meterAt(view, section.start);
  if (
    ![bar, beat, tick].every(Number.isSafeInteger) ||
    bar < 1 ||
    beat < 1 ||
    beat > meter.numerator ||
    tick >= meter.beat
  ) {
    throw Error("Time is outside the bar or beat range.");
  }
  if ([draft.pitch, draft.velocity, draft.duration].some((v) => !v.trim()))
    throw Error("Use whole numbers for pitch, velocity and duration.");
  const update = {
    id,
    start:
      section.start + (bar - 1) * meter.bar + (beat - 1) * meter.beat + tick,
    duration: Number(draft.duration),
    pitch: Number(draft.pitch),
    velocity: Number(draft.velocity),
  };
  validateDraft([update], section);
  return update;
}
