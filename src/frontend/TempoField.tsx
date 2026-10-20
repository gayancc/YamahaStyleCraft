const MIN_BPM = 20;
const MAX_BPM = 400;

function clampBpm(value: number): number {
  return Math.min(MAX_BPM, Math.max(MIN_BPM, value));
}

/** FL Studio tempo field: a light rounded box with large dark digits, the fraction set small
 * (130.000), and a stacked up/down spinner on the right edge. Wheel and arrow keys step by one
 * BPM; typing commits on blur. The real input stays in the box so it keeps native editing and
 * the spinbutton role; the large face is drawn over it until it takes focus. */
export function TempoField({
  tempo,
  revision,
  disabled,
  onCommit,
}: {
  tempo: number | null;
  revision: number;
  disabled: boolean;
  onCommit: (bpm: number) => void;
}) {
  const parts = tempo == null ? null : tempo.toFixed(3).split(".");
  const step = (delta: number) => {
    if (tempo == null || disabled) return;
    const next = Math.round(clampBpm(tempo + delta) * 1000) / 1000;
    if (Math.abs(next - tempo) > 0.0005) onCommit(next);
  };
  return (
    <div
      className="tempo-field"
      data-disabled={disabled ? "true" : undefined}
      data-hint="Tempo"
      title="Tempo (BPM). Wheel or Up/Down changes by 1."
      onWheel={(e) => step(e.deltaY < 0 ? 1 : -1)}
    >
      <input
        aria-label="Tempo BPM"
        type="number"
        min={MIN_BPM}
        max={MAX_BPM}
        step={0.01}
        disabled={disabled}
        key={tempo == null ? "t" : `t-${revision}-${tempo}`}
        defaultValue={tempo == null ? undefined : Number(tempo.toFixed(2))}
        onKeyDown={(e) => {
          if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            e.preventDefault();
            step(e.key === "ArrowUp" ? 1 : -1);
          }
        }}
        onBlur={(e) => {
          if (tempo == null) return;
          const n = Number(e.target.value);
          if (
            Number.isFinite(n) &&
            n >= MIN_BPM &&
            n <= MAX_BPM &&
            Math.abs(n - tempo) > 0.001
          ) {
            onCommit(n);
          } else {
            e.target.value = String(Number(tempo.toFixed(2)));
          }
        }}
      />
      {parts && (
        <span className="tempo-face" aria-hidden="true">
          {parts[0]}
          <small>.{parts[1]}</small>
        </span>
      )}
      <span className="tempo-spin">
        <button
          type="button"
          tabIndex={-1}
          aria-label="Increase tempo"
          disabled={disabled || tempo == null}
          onClick={() => step(1)}
        />
        <button
          type="button"
          tabIndex={-1}
          aria-label="Decrease tempo"
          disabled={disabled || tempo == null}
          onClick={() => step(-1)}
        />
      </span>
    </div>
  );
}
