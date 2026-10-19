// Independent geometry for the vertical keyboard. Coordinates share the note
// grid's semitone axis; natural key faces continue behind the accidentals.
export const isBlackKey = (pitch: number) =>
  [1, 3, 6, 8, 10].includes(pitch % 12);

const naturalEdges: Record<number, [number, number]> = {
  0: [0, 1.5],
  2: [1.5, 3.5],
  4: [3.5, 5],
  5: [5, 6.5],
  7: [6.5, 8.5],
  9: [8.5, 10.5],
  11: [10.5, 12],
};
export interface PianoKeyFace {
  pitch: number;
  black: boolean;
  top: number;
  bottom: number;
  right: number;
}

export function pianoKeyFaces(row: number, width: number): PianoKeyFace[] {
  const blackWidth = Math.round(((width - 4) * 2) / 3);
  const faces = Array.from({ length: 128 }, (_, pitch) => {
    const black = isBlackKey(pitch),
      octave = Math.floor(pitch / 12) * 12,
      [low, high] = black
        ? [pitch, pitch + 1]
        : naturalEdges[pitch % 12].map((edge) => octave + edge);
    return {
      pitch,
      black,
      // Round shared boundaries, not individual heights: odd row sizes cannot
      // accumulate gaps between octaves. Trim the incomplete top octave at G9.
      top: Math.round((128 - Math.min(128, high)) * row),
      bottom: Math.round((128 - Math.max(0, low)) * row),
      right: black ? blackWidth : width - 1,
    };
  });
  return faces.sort((a, b) => Number(a.black) - Number(b.black));
}

export function pianoKeyAt(
  x: number,
  contentY: number,
  row: number,
  width: number,
): number | null {
  if (x < 0 || x >= width || contentY < 0 || contentY >= 128 * row) return null;
  // Reverse paint order: an accidental wins over the white face underneath.
  const faces = pianoKeyFaces(row, width);
  for (let i = faces.length - 1; i >= 0; i--) {
    const face = faces[i];
    if (x < face.right && contentY >= face.top && contentY < face.bottom)
      return face.pitch;
  }
  return null;
}

export function drawPianoKeyboard(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  row: number,
  ruler: number,
  scrollY: number,
  label: (pitch: number) => string,
) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, ruler, width, Math.max(0, height - ruler));
  ctx.clip();
  for (const face of pianoKeyFaces(row, width)) {
    const top = ruler + face.top - scrollY,
      bottom = ruler + face.bottom - scrollY;
    if (bottom <= ruler || top >= height) continue;
    const octaveC = face.pitch % 12 === 0;
    const gradient = ctx.createLinearGradient(0, 0, face.right, 0);
    gradient.addColorStop(
      0,
      face.black ? "#292a2c" : octaveC ? "#a3a7ae" : "#c2c6cd",
    );
    gradient.addColorStop(
      1,
      face.black ? "#505153" : octaveC ? "#c9cdd4" : "#e9edf4",
    );
    ctx.fillStyle = gradient;
    ctx.fillRect(0, top, face.right, bottom - top);
    if (!face.black && (octaveC || face.pitch % 12 === 5)) {
      ctx.fillStyle = "#92969c";
      ctx.fillRect(0, bottom - 1, face.right, 1);
    }
    if (octaveC) {
      // Labels follow the C semitone row, not its extended white-key bounds.
      ctx.fillStyle = "#36536c";
      ctx.font = "10px Segoe UI";
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(
        label(face.pitch),
        width - 3,
        ruler + (127.5 - face.pitch) * row - scrollY,
      );
    }
  }
  ctx.fillStyle = "#4b5962";
  ctx.fillRect(width - 1, ruler, 1, height - ruler);
  ctx.restore();
}
