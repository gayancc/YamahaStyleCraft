import { describe, expect, it } from "vitest";
import { liveSwitchSteps } from "./liveSwitch";

const sections = [
  "Intro 1",
  "Main A",
  "Main B",
  "Fill In AA",
  "Fill In BB",
  "Fill In BA",
  "Ending 1",
].map((label, i) => ({ id: i + 1, label }));
const sec = (label: string) => sections.find((s) => s.label === label)!;
const all = () => true;

describe("liveSwitchSteps", () => {
  it("plays the destination fill once, then loops the Main", () => {
    expect(liveSwitchSteps(sections, sec("Main B"), "Main A", all)).toEqual([
      { id: sec("Fill In BB").id, looping: false },
      { id: sec("Main B").id, looping: true },
    ]);
  });
  it("skips an empty fill", () => {
    expect(
      liveSwitchSteps(
        sections,
        sec("Main B"),
        "Main A",
        (id) => id !== sec("Fill In BB").id,
      ),
    ).toEqual([{ id: sec("Main B").id, looping: true }]);
  });
  it("a fill is one-shot and returns to its Main", () => {
    expect(liveSwitchSteps(sections, sec("Fill In BB"), "Main A", all)).toEqual([
      { id: sec("Fill In BB").id, looping: false },
      { id: sec("Main B").id, looping: true },
    ]);
  });
  it("break returns to the last Main", () => {
    expect(liveSwitchSteps(sections, sec("Fill In BA"), "Main B", all)).toEqual([
      { id: sec("Fill In BA").id, looping: false },
      { id: sec("Main B").id, looping: true },
    ]);
  });
  it("intro returns to the selected Main and ending stops", () => {
    expect(liveSwitchSteps(sections, sec("Intro 1"), "Main B", all)).toEqual([
      { id: sec("Intro 1").id, looping: false },
      { id: sec("Main B").id, looping: true },
    ]);
    expect(liveSwitchSteps(sections, sec("Ending 1"), "Main B", all)).toEqual([
      { id: sec("Ending 1").id, looping: false },
    ]);
  });
});
