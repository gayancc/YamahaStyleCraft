export interface TimedNote {
  start: number;
  duration: number;
}

/** Note density across a section: how many notes sound in each of `bins` equal slices, scaled
 * 0..1 against the busiest slice. Drives the little activity strip on each Style part. */
export function activityBins(
  notes: readonly TimedNote[],
  start: number,
  end: number,
  bins = 24,
): number[] {
  const span = end - start;
  const counts = new Array<number>(bins).fill(0);
  if (!(span > 0) || bins < 1) return counts;
  for (const note of notes) {
    const from = Math.max(0, Math.floor(((note.start - start) / span) * bins));
    const to = Math.min(
      bins - 1,
      Math.floor(((note.start + Math.max(note.duration, 1) - 1 - start) / span) * bins),
    );
    for (let i = from; i <= to; i += 1) counts[i] += 1;
  }
  const peak = Math.max(...counts);
  return peak > 0 ? counts.map((c) => c / peak) : counts;
}
