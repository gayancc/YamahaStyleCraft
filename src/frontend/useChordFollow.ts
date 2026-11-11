import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Recognized = { root: number; quality: string };
type XfChord = Recognized & { seq: number; bass: number | null; bassQuality?: string };

/** Left-hand zone, inclusive (Genos default split point B3). */
const LEFT_HAND_MAX = 59;
const POLL_MS = 60;
const ROOTS = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

interface ChordFollowOptions {
  /** Live Play on and the style is sounding. */
  enabled: boolean;
  inputName: string | undefined;
  monitoring: boolean;
  singleFinger?: boolean;
  setMonitoring: (active: boolean) => void;
  docLabel: string | undefined;
  onError: (message: string) => void;
}

/**
 * Live style play: recognize the chord held in the keyboard's left hand and tell the
 * transport to follow it. The chord stays until a different one is played, like the keyboard.
 */
export function useChordFollow({
  enabled,
  inputName,
  monitoring,
  singleFinger = false,
  setMonitoring,
  docLabel,
  onError,
}: ChordFollowOptions): string | null {
  const [heard, setHeard] = useState<string | null>(null);
  const startedHere = useRef(false);
  const lastSent = useRef("");
  const fail = useRef(onError);
  fail.current = onError;

  // Listen to the keyboard while following; release it again afterwards.
  useEffect(() => {
    if (!enabled || !inputName || monitoring) return;
    let cancelled = false;
    void invoke("start_monitor", { name: inputName, docLabel })
      .then(() => {
        if (cancelled) return;
        startedHere.current = true;
        setMonitoring(true);
      })
      .catch((e) => {
        if (!cancelled) fail.current(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, inputName, monitoring]);

  useEffect(() => {
    if (enabled) return;
    lastSent.current = "";
    void invoke("set_live_chord", {
      docLabel,
      root: null,
      quality: null,
      bass: null,
    }).catch(() => undefined);
    if (startedHere.current) {
      startedHere.current = false;
      void invoke("stop_monitor", { docLabel })
        .then(() => setMonitoring(false))
        .catch(() => undefined);
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled || !monitoring) return;
    let live = true;
    let inFlight = false;
    lastSent.current = "";
    const send = async (
      root: number,
      quality: string,
      bass: number | null,
      source: string,
      bassQuality?: string,
    ) => {
      const key = `${root}:${quality}:${bass}:${bassQuality ?? ""}`;
      if (key === lastSent.current) return;
      await invoke("set_live_chord", { root, quality, bass, docLabel, ...(bassQuality && { bassQuality }) });
      if (!live) return;
      lastSent.current = key;
      setHeard(`${ROOTS[root]} ${quality} (${source})`);
    };
    const tick = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        // The keyboard's own arranger reports the chord it detected (XF chord message).
        const xf = await invoke<XfChord | null>("monitor_xf_chord", { docLabel });
        if (!live) return;
        if (xf) {
          await send(xf.root, xf.quality, xf.bass, "keyboard", xf.bassQuality);
          return;
        }
        if (singleFinger) {
          const chord = await invoke<Recognized | null>("monitor_single_finger_chord", { docLabel });
          if (!live) return;
          if (chord) {
            await send(chord.root, chord.quality, null, "notes");
            return;
          }
        }
        const notes = await invoke<number[]>("monitor_held_notes", {
          docLabel,
          channel: null,
          maxPitch: LEFT_HAND_MAX,
        });
        if (!live) return;
        if (notes.length === 0) {
          const all = await invoke<number[]>("monitor_held_notes", {
            docLabel,
            channel: null,
            maxPitch: null,
          });
          if (live && all.length > 0)
            setHeard("Notes above B3 are ignored — play the chord lower");
          return;
        }
        if (singleFinger) return;
        const hit = await invoke<Recognized | null>("recognize_chord_preview", {
          notes,
        });
        if (!live) return;
        if (!hit) {
          setHeard(`Unrecognized chord (${notes.length} notes)`);
          return;
        }
        const lowest = Math.min(...notes) % 12;
        const bass = lowest === hit.root ? null : lowest;
        await send(hit.root, hit.quality, bass, "notes");
      } catch (e) {
        if (live) fail.current(String(e));
      } finally {
        inFlight = false;
      }
    };
    const id = window.setInterval(() => void tick(), POLL_MS);
    return () => {
      live = false;
      window.clearInterval(id);
    };
  }, [enabled, monitoring, docLabel, singleFinger]);

  if (!enabled) return null;
  if (!inputName) return "Select a MIDI input in Recording / Monitor";
  if (!monitoring) return "Starting MIDI input…";
  return heard ?? "Listening — play a chord in the left hand";
}
