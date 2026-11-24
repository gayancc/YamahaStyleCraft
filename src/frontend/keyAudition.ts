import { invoke } from "@tauri-apps/api/core";

const DRUM_CHANNELS = [9, 10];
const DRUM_MIN = 13;
const DRUM_MAX = 91;

/** Drum channels only sound notes 13..91 on the Genos shell (see preview_drum_note). */
export function isAuditionable(channel: number, pitch: number): boolean {
  if (pitch < 0 || pitch > 127) return false;
  if (DRUM_CHANNELS.includes(channel))
    return pitch >= DRUM_MIN && pitch <= DRUM_MAX;
  return true;
}

export interface KeyOutput {
  index: number;
  name: string;
}

/**
 * Press-and-hold audition. Remembers the channel each held pitch started on so
 * the matching note-off is always delivered, even if the part changes mid-hold.
 */
export function createKeyAuditioner(
  onError: (message: string) => void,
  call: typeof invoke = invoke,
) {
  const held = new Map<number, number>();
  const fail = (e: unknown) => onError(String(e));
  return {
    on(
      output: KeyOutput,
      channel: number,
      pitch: number,
      velocity: number,
      docLabel?: string,
    ) {
      if (!isAuditionable(channel, pitch)) return;
      held.set(pitch, channel);
      void call("key_note_on", {
        index: output.index,
        name: output.name,
        channel,
        pitch,
        velocity,
        docLabel,
      }).catch((e) => {
        held.delete(pitch);
        fail(e);
      });
    },
    off(pitch: number) {
      const channel = held.get(pitch);
      if (channel === undefined) return;
      held.delete(pitch);
      void call("key_note_off", { channel, pitch }).catch(fail);
    },
    releaseAll() {
      held.clear();
      void call("key_release_all").catch(fail);
    },
    get heldCount() {
      return held.size;
    },
  };
}
