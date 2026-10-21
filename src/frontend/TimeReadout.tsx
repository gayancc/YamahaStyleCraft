import { useEffect, useRef, useState } from "react";

/** Minutes, seconds and milliseconds as drawn in the readout. */
export function formatElapsed(ms: number): {
  minutes: string;
  seconds: string;
  millis: string;
} {
  const total = Math.max(0, Math.floor(ms));
  return {
    minutes: Math.floor(total / 60000).toString().padStart(2, "0"),
    seconds: Math.floor((total / 1000) % 60).toString().padStart(2, "0"),
    millis: (total % 1000).toString().padStart(3, "0"),
  };
}

/** Largest disagreement (ms) between the running clock and a status poll that is ignored, so
 * poll jitter never makes the digits step backwards or stutter. */
const SNAP_AFTER_MS = 80;

/** The elapsed-time readout. The engine reports elapsed time a few times a second; between reports
 * the display keeps counting on the animation clock, and only re-aligns when it has drifted. */
export function TimeReadout({
  elapsedMs,
  playing,
}: {
  elapsedMs: number;
  playing: boolean;
}) {
  const [shown, setShown] = useState(elapsedMs);
  const base = useRef({ elapsed: elapsedMs, at: performance.now() });

  useEffect(() => {
    const now = performance.now();
    const predicted = base.current.elapsed + (playing ? now - base.current.at : 0);
    if (!playing || Math.abs(elapsedMs - predicted) > SNAP_AFTER_MS) {
      base.current = { elapsed: elapsedMs, at: now };
      setShown(elapsedMs);
    }
  }, [elapsedMs, playing]);

  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      const { elapsed, at } = base.current;
      setShown(elapsed + (performance.now() - at));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  const t = formatElapsed(shown);
  return (
    <div className="time-readout" role="timer" aria-label="Elapsed time">
      <Digits text={t.minutes} />:<Digits text={t.seconds} />.
      <span>
        <Digits text={t.millis} />
      </span>
    </div>
  );
}

/** Every digit sits in a fixed-width cell, so changing digits never move the text sideways. */
function Digits({ text }: { text: string }) {
  return (
    <>
      {[...text].map((digit, i) => (
        <i className="digit" key={i}>
          {digit}
        </i>
      ))}
    </>
  );
}
