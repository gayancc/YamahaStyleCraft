import type { Note, NoteInput, Section } from "./contracts";
import { validateDraft } from "./pianoEditing";

export type NoteClipboard = { ppq: number; notes: Note[] };
const validPpq = (value: unknown): value is number =>
  Number.isInteger(value) && Number(value) >= 1 && Number(value) <= 32767;

/** Validate the cross-window bridge before using its timing or note fields. */
export function isNoteClipboard(value: unknown): value is NoteClipboard {
  if (!value || typeof value !== "object") return false;
  const p = value as Partial<NoteClipboard>;
  return (
    validPpq(p.ppq) &&
    Array.isArray(p.notes) &&
    p.notes.length <= 50_000 &&
    p.notes.every(
      (n) =>
        n &&
        [
          n.id,
          n.offId,
          n.track,
          n.channel,
          n.start,
          n.duration,
          n.pitch,
          n.velocity,
        ].every(Number.isSafeInteger) &&
        n.id >= 0 &&
        n.offId >= 0 &&
        n.track >= 0 &&
        n.track < 65536 &&
        n.channel >= 1 &&
        n.channel <= 16 &&
        n.start >= 0 &&
        n.duration >= 1 &&
        n.start + n.duration <= 1_000_000_000 &&
        n.pitch >= 0 &&
        n.pitch <= 127 &&
        n.velocity >= 1 &&
        n.velocity <= 127,
    )
  );
}

/** Preserve musical intervals, independently rounding relative onset and length. */
export function planClipboardPaste(
  clipboard: NoteClipboard,
  destination: {
    ppq: number;
    section: Section;
    cursor: number;
    channel: number;
    track: number;
  },
): {
  notes: NoteInput[];
  shifted: boolean;
  omitted: number;
  clipped: number;
  start: number;
} {
  if (!isNoteClipboard(clipboard) || !validPpq(destination.ppq)) {
    throw new Error("Invalid note clipboard timing. Copy the notes again.");
  }
  if (
    !Number.isSafeInteger(destination.cursor) ||
    destination.cursor < 0 ||
    !Number.isSafeInteger(destination.channel) ||
    destination.channel < 1 ||
    destination.channel > 16 ||
    !Number.isSafeInteger(destination.track) ||
    destination.track < 0 ||
    destination.track >= 65536
  ) {
    throw new Error("Invalid note paste destination.");
  }
  if (!clipboard.notes.length)
    return { notes: [], shifted: false, omitted: 0, clipped: 0, start: 0 };
  const origin = clipboard.notes.reduce(
    (first, n) => Math.min(first, n.start),
    Infinity,
  );
  const ratio = destination.ppq / clipboard.ppq;
  const sectionLength = destination.section.end - destination.section.start;
  if (!Number.isSafeInteger(sectionLength) || sectionLength < 1)
    throw new Error("Invalid paste section.");
  const relative = clipboard.notes.map((n) => ({
    n,
    tick: Math.round((n.start - origin) * ratio),
    duration: Math.max(1, Math.round(n.duration * ratio)),
  }));
  const span = relative.reduce(
    (end, n) => Math.max(end, n.tick + n.duration),
    0,
  );
  const requested = Math.min(destination.cursor, sectionLength - 1);
  const start =
    span <= sectionLength
      ? Math.min(requested, sectionLength - span)
      : requested;
  let clipped = 0;
  const result = relative
    .filter((n) => start + n.tick < sectionLength)
    .map(({ n, tick, duration }) => {
      const bounded = Math.min(duration, sectionLength - start - tick);
      if (bounded < duration) clipped++;
      return {
        track: destination.track,
        channel: destination.channel,
        start: destination.section.start + start + tick,
        duration: bounded,
        pitch: n.pitch,
        velocity: n.velocity,
      };
    });
  validateDraft(result, destination.section);
  return {
    notes: result,
    start,
    shifted: start < requested,
    omitted: clipboard.notes.length - result.length,
    clipped,
  };
}
