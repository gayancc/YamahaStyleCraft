/**
 * Live style play: what a Style section button queues, matching the keyboard.
 * Fill-ins, Break and Intro play once. Only a Main loops. The transport times the switch.
 */
export interface LiveSection {
  id: number;
  label: string;
}
export interface LiveStep {
  id: number;
  looping: boolean;
}

const MAIN = /^main ([a-d])$/;
const FILL = /^fill (?:in ([a-d])\1|([a-d]))$/;
const marker = (label: string) => label.trim().toLowerCase().replace(/\s+/g, " ");
const fillLetter = (label: string) => {
  const match = FILL.exec(marker(label));
  return match?.[1] ?? match?.[2];
};

const byLabel = (sections: LiveSection[], label: string) =>
  sections.find((s) => marker(s.label) === marker(label));

export function isMainLabel(label: string): boolean {
  return MAIN.test(marker(label));
}

/** The Main a one-shot section hands back to, or null for Ending (stops afterwards). */
export function returnMainLabel(
  label: string,
  lastMain: string,
): string | null {
  const fill = fillLetter(label);
  if (fill) return `Main ${fill.toUpperCase()}`;
  if (marker(label).startsWith("ending ")) return null;
  return lastMain;
}

export function liveSwitchSteps(
  sections: LiveSection[],
  pressed: LiveSection,
  lastMain: string,
  hasNotes: (id: number) => boolean,
): LiveStep[] {
  const main = MAIN.exec(marker(pressed.label));
  if (main) {
    // Auto fill-in: the fill for the destination Main, then the Main itself.
    const fill = sections.find((section) => fillLetter(section.label) === main[1]);
    return [
      ...(fill && hasNotes(fill.id) ? [{ id: fill.id, looping: false }] : []),
      { id: pressed.id, looping: true },
    ];
  }
  const back = returnMainLabel(pressed.label, lastMain);
  const target = back
    ? byLabel(sections, back) ?? sections.find((section) => isMainLabel(section.label))
    : undefined;
  return [
    { id: pressed.id, looping: false },
    ...(target ? [{ id: target.id, looping: true }] : []),
  ];
}
