import type { QuantizeOptions } from "./contracts";

const BEAT = 480;
/** Four sample note starts inside one beat, deliberately off the grid. */
const SAMPLE_STARTS = [34, 150, 262, 391];
const WIDTH = 300;
const TOP_Y = 13;
const BOTTOM_Y = 43;

export interface PreviewNote {
  raw: number;
  snapped: number;
}

/** Grid line positions (ticks) across one beat for the chosen size, tuplet and swing. */
export function gridLines(options: QuantizeOptions): number[] {
  const size = options.size.startsWith("mixed")
    ? Number(options.size === "mixed8" ? 240 : 120)
    : Number(options.size);
  const base =
    options.tuplet > 0
      ? (size * (options.tuplet === 3 ? 2 : 4)) / options.tuplet
      : size;
  const lines: number[] = [];
  const swingShift =
    options.tuplet > 0 || options.size.startsWith("mixed") || options.target === "lengths"
      ? 0
      : (base / 2) * (options.swing / 100);
  for (let i = 0; i * base <= BEAT + 0.5; i += 1) {
    lines.push(i * base + (i % 2 === 1 ? swingShift : 0));
  }
  if (options.size.startsWith("mixed")) {
    const third = (options.size === "mixed8" ? 160 : 80);
    for (let t = 0; t <= BEAT; t += third) lines.push(t);
  }
  return [...new Set(lines.map((l) => Math.round(l * 100) / 100))]
    .filter((l) => l <= BEAT)
    .sort((a, b) => a - b);
}

/** Where each sample note lands: strength moves it part-way when Soft is on, the catch range
 * leaves far-away notes alone, and the safe range leaves near-grid notes alone. */
export function previewNotes(options: QuantizeOptions): PreviewNote[] {
  const lines = gridLines(options);
  const step = lines.length > 1 ? Math.min(...lines.slice(1).map((l, i) => l - lines[i])) : BEAT;
  return SAMPLE_STARTS.map((raw) => {
    const nearest = lines.reduce((a, b) => (Math.abs(b - raw) < Math.abs(a - raw) ? b : a), lines[0]);
    const distance = Math.abs(nearest - raw);
    const inCatch = !options.catchEnabled || distance <= (step / 2) * (options.catchPercent / 100);
    const outsideSafe = distance > options.safeRange;
    if (!inCatch || !outsideSafe) return { raw, snapped: raw };
    const pull = options.soft ? options.strength / 100 : 1;
    return { raw, snapped: raw + (nearest - raw) * pull };
  });
}

const x = (ticks: number) => 8 + (ticks / BEAT) * (WIDTH - 16);

/** A schematic of what the current settings do to one beat: the grid, the swing, and four sample
 * notes sliding from where they were played (hollow) to where they would land (filled). */
export function QuantizePreview({ options }: { options: QuantizeOptions }) {
  const lines = gridLines(options);
  const notes = previewNotes(options);
  return (
    <svg
      className="quantize-preview"
      viewBox={`0 0 ${WIDTH} 56`}
      role="img"
      aria-label="Schematic preview of the quantize grid on one beat"
    >
      {lines.map((l, i) => (
        <line
          key={l}
          x1={x(l)}
          x2={x(l)}
          y1={4}
          y2={52}
          className={i === 0 || l === BEAT ? "qp-beat" : "qp-grid"}
        />
      ))}
      {notes.map((n) => (
        <g key={n.raw}>
          <line className="qp-link" x1={x(n.raw)} y1={TOP_Y + 4} x2={x(n.snapped)} y2={BOTTOM_Y - 4} />
          <circle className="qp-raw" cx={x(n.raw)} cy={TOP_Y} r={4} />
          <circle className="qp-snap" cx={x(n.snapped)} cy={BOTTOM_Y} r={4} />
        </g>
      ))}
    </svg>
  );
}
