import { describe, expect, it, vi } from "vitest";
import { createKeyAuditioner, isAuditionable } from "./keyAudition";
import { keyVelocity, pitchAtRow, soundingKeys } from "./pianoKeyboard";

const out = { index: 2, name: "Genos OUT" };
const make = () => {
  const call = vi.fn().mockResolvedValue(undefined);
  const errors: string[] = [];
  const a = createKeyAuditioner((m) => errors.push(m), call as never);
  return { a, call, errors };
};

describe("key audition", () => {
  it("limits drum channels to 13..91 and allows melodic anywhere", () => {
    expect(isAuditionable(10, 36)).toBe(true);
    expect(isAuditionable(10, 12)).toBe(false);
    expect(isAuditionable(9, 92)).toBe(false);
    expect(isAuditionable(3, 0)).toBe(true);
    expect(isAuditionable(3, 127)).toBe(true);
  });

  it("sends note-on then note-off on the channel the key started on", () => {
    const { a, call } = make();
    a.on(out, 4, 60, 90, "main");
    expect(call).toHaveBeenCalledWith("key_note_on", {
      index: 2,
      name: "Genos OUT",
      channel: 4,
      pitch: 60,
      velocity: 90,
      docLabel: "main",
    });
    a.off(60);
    expect(call).toHaveBeenLastCalledWith("key_note_off", {
      channel: 4,
      pitch: 60,
    });
    expect(a.heldCount).toBe(0);
  });

  it("ignores unmapped drum keys and stray releases", () => {
    const { a, call } = make();
    a.on(out, 10, 5, 90);
    a.off(5);
    expect(call).not.toHaveBeenCalled();
  });

  it("surfaces backend errors and forgets the failed note", async () => {
    const call = vi.fn().mockRejectedValue("MMSYSERR_ALLOCATED");
    const errors: string[] = [];
    const a = createKeyAuditioner((m) => errors.push(m), call as never);
    a.on(out, 1, 60, 90);
    await Promise.resolve();
    await Promise.resolve();
    expect(errors).toEqual(["MMSYSERR_ALLOCATED"]);
    expect(a.heldCount).toBe(0);
  });

  it("releaseAll clears held notes", () => {
    const { a, call } = make();
    a.on(out, 1, 60, 90);
    a.releaseAll();
    expect(call).toHaveBeenLastCalledWith("key_release_all");
    expect(a.heldCount).toBe(0);
  });
});

describe("key feel helpers", () => {
  it("raises velocity from the left to the right edge of the key", () => {
    expect(keyVelocity(0, 60)).toBe(1);
    expect(keyVelocity(59, 60)).toBe(127);
    expect(keyVelocity(-5, 60)).toBe(1);
    expect(keyVelocity(500, 60)).toBe(127);
    expect(keyVelocity(30, 60)).toBeGreaterThan(keyVelocity(10, 60));
  });
  it("follows a held key by row, even outside the key column", () => {
    expect(pitchAtRow(0, 18)).toBe(127);
    expect(pitchAtRow(18 * 67 + 5, 18)).toBe(60);
    expect(pitchAtRow(-1, 18)).toBeNull();
    expect(pitchAtRow(128 * 18, 18)).toBeNull();
  });
  it("finds the pitches of notes sounding at a tick", () => {
    const notes = [
      { pitch: 60, velocity: 50, start: 0, duration: 10 },
      { pitch: 60, velocity: 90, start: 5, duration: 10 },
      { pitch: 64, velocity: 70, start: 10, duration: 10 },
    ];
    expect([...soundingKeys(notes, 7)]).toEqual([60]);
    expect([...soundingKeys(notes, 10)].sort()).toEqual([60, 64]);
    expect(soundingKeys(notes, null).size).toBe(0);
    expect(soundingKeys(notes, 20).size).toBe(0);
  });
});
