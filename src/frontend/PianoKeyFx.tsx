import { useMemo } from "react";
import { pianoKeyFaces } from "./pianoKeyboard";

/**
 * Key press feedback laid over the canvas keyboard. Matches FL Studio's piano
 * roll: a lit key is repainted instantly and restored instantly (no easing).
 * `held` = key under the mouse, `playing` = keys sounding in transport.
 */
export function PianoKeyFx({
  row,
  width,
  top,
  height,
  scrollY,
  held,
  playing,
}: {
  row: number;
  width: number;
  top: number;
  height: number;
  scrollY: number;
  held: ReadonlySet<number>;
  playing: ReadonlySet<number>;
}) {
  const faces = useMemo(() => pianoKeyFaces(row, width), [row, width]);
  const lit = (pitch: number) => held.has(pitch) || playing.has(pitch);
  return (
    <div
      className="piano-key-fx"
      aria-hidden="true"
      data-testid="piano-key-fx"
      style={{ top, width, height }}
    >
      {faces.map((face) => {
        const y = face.top - scrollY;
        if (y + (face.bottom - face.top) < 0 || y > height) return null;
        const on = lit(face.pitch);
        // A lit white key extends behind its accidentals; keep those dark.
        const mask =
          face.black && !on && (lit(face.pitch - 1) || lit(face.pitch + 1));
        return (
          <div
            key={face.pitch}
            data-pitch={face.pitch}
            className={`piano-key-fx-key${face.black ? " black" : ""}${on ? " on" : ""}${mask ? " mask" : ""}`}
            style={{
              top: y,
              height: face.bottom - face.top,
              width: face.right,
            }}
          />
        );
      })}
    </div>
  );
}
