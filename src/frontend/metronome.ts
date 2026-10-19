/** PC-audio metronome click for playback (SFM-043). Not MIDI, not exported. */

let ctx: AudioContext | null = null;
let lastBeat = -1;

function audio(): AudioContext {
  if (!ctx) ctx = new AudioContext();
  return ctx;
}

export function resetMetronome() {
  lastBeat = -1;
}

export function clickMetronome(accent: boolean) {
  const ac = audio();
  if (ac.state === "suspended") void ac.resume();
  const t0 = ac.currentTime;
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = "square";
  osc.frequency.value = accent ? 1320 : 880;
  gain.gain.setValueAtTime(accent ? 0.12 : 0.07, t0);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.05);
  osc.connect(gain);
  gain.connect(ac.destination);
  osc.start(t0);
  osc.stop(t0 + 0.055);
}

/** Play `bars * numerator` count-in clicks at `tempoBpm`, then resolve. */
export async function playCountIn(
  bars: number,
  tempoBpm: number,
  numerator: number,
): Promise<void> {
  const n = Math.max(0, Math.min(8, Math.round(bars)));
  if (n <= 0 || !(tempoBpm > 0) || numerator < 1) return;
  const beatMs = 60_000 / tempoBpm;
  const beats = n * numerator;
  resetMetronome();
  for (let i = 0; i < beats; i++) {
    clickMetronome(i % numerator === 0);
    await new Promise((r) => setTimeout(r, beatMs));
  }
}

/** Fire a click when the playhead crosses a new beat boundary. */
export function metronomeAtTick(
  tick: number,
  sectionStart: number,
  ppq: number,
  numerator: number,
  denominator: number,
) {
  if (ppq <= 0 || numerator <= 0 || denominator <= 0) return;
  const beatTicks = (ppq * 4) / denominator;
  if (beatTicks <= 0) return;
  const local = Math.max(0, tick - sectionStart);
  const beat = Math.floor(local / beatTicks);
  if (beat === lastBeat || beat < 0) return;
  lastBeat = beat;
  clickMetronome(beat % numerator === 0);
}
