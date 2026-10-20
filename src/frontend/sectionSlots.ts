/** Genos panel layout for the section bar.
 *
 * The Genos panel always offers Intro 1-3, Main A-D, Fill A-D, Break and Ending 1-3. A loaded Style
 * only provides some of them, so the bar renders every slot and leaves the ones without data empty
 * and inactive. Yamaha Style files name sections with letters (Intro A, Fill In AA, Fill In BA, ...);
 * those names are mapped onto the panel slots. Sections with no slot stay reachable in `extra`. */
export interface SectionLike {
  readonly id: number;
  readonly label: string;
  readonly start: number;
  readonly end: number;
}

export type SlotGroupName = "Intro" | "Main" | "Fill" | "Break" | "Ending";

export interface SectionSlot<T extends SectionLike> {
  readonly key: string;
  readonly label: string;
  /** Marker name written into the Style when this slot is added (for example `Fill In BA`). */
  readonly yamahaLabel: string;
  readonly section: T | null;
}

export interface SectionSlotGroup<T extends SectionLike> {
  readonly name: SlotGroupName;
  readonly slots: readonly SectionSlot<T>[];
}

export interface SectionLayout<T extends SectionLike> {
  readonly groups: readonly SectionSlotGroup<T>[];
  readonly extra: readonly T[];
}

const PANEL: readonly {
  name: SlotGroupName;
  slots: readonly [key: string, label: string, yamahaLabel: string][];
}[] = [
  {
    name: "Intro",
    slots: [
      ["intro1", "Intro 1", "Intro A"],
      ["intro2", "Intro 2", "Intro B"],
      ["intro3", "Intro 3", "Intro C"],
    ],
  },
  {
    name: "Main",
    slots: [
      ["mainA", "Main A", "Main A"],
      ["mainB", "Main B", "Main B"],
      ["mainC", "Main C", "Main C"],
      ["mainD", "Main D", "Main D"],
    ],
  },
  {
    name: "Fill",
    slots: [
      ["fillA", "Fill A", "Fill In AA"],
      ["fillB", "Fill B", "Fill In BB"],
      ["fillC", "Fill C", "Fill In CC"],
      ["fillD", "Fill D", "Fill In DD"],
    ],
  },
  { name: "Break", slots: [["break", "Break", "Fill In BA"]] },
  {
    name: "Ending",
    slots: [
      ["ending1", "Ending 1", "Ending A"],
      ["ending2", "Ending 2", "Ending B"],
      ["ending3", "Ending 3", "Ending C"],
    ],
  },
];

/** Default and maximum length in bars for a new section: SFM Style Studio's section setup uses
 * 4 bars for main, intro and ending, 1 bar for fills and the break, and caps fills and the break at 8. */
export function defaultSlotBars(group: SlotGroupName): { bars: number; max: number } {
  return group === "Fill" || group === "Break" ? { bars: 1, max: 8 } : { bars: 4, max: 32 };
}

const LETTER_TO_NUMBER: Readonly<Record<string, string>> = { a: "1", b: "2", c: "3", "1": "1", "2": "2", "3": "3" };

/** Panel slot key for a Style section name, or null when the name has no Genos panel slot. */
export function slotKeyFor(label: string): string | null {
  const text = label.trim().toLowerCase().replace(/\s+/g, " ");
  let m = /^(intro|ending) ([abc123])$/.exec(text);
  if (m) return `${m[1]}${LETTER_TO_NUMBER[m[2]]}`;
  m = /^main ([a-d])$/.exec(text);
  if (m) return `main${m[1].toUpperCase()}`;
  m = /^fill in ([a-d])\1$/.exec(text);
  if (m) return `fill${m[1].toUpperCase()}`;
  if (text === "fill in ba" || text === "break" || text === "break fill") return "break";
  return null;
}

/** Arrange a Style's sections into the fixed Genos panel layout. */
export function layoutSections<T extends SectionLike>(sections: readonly T[]): SectionLayout<T> {
  const bySlot = new Map<string, T>();
  const extra: T[] = [];
  for (const section of sections) {
    const key = slotKeyFor(section.label);
    if (key && !bySlot.has(key)) bySlot.set(key, section);
    else extra.push(section);
  }
  return {
    groups: PANEL.map((group) => ({
      name: group.name,
      slots: group.slots.map(([key, label, yamahaLabel]) => ({
        key,
        label,
        yamahaLabel,
        section: bySlot.get(key) ?? null,
      })),
    })),
    extra,
  };
}

/** CSS grid columns for the section bar: every pad gets an identical track and each group is
 * separated by one fixed gutter, so all pads share one width whatever the group sizes. */
export function sectionGridColumns<T extends SectionLike>(
  layout: SectionLayout<T>,
  pad = "minmax(0, 64px)",
  gutter = "9px",
): string {
  const counts = [...layout.groups.map((g) => g.slots.length), layout.extra.length].filter(
    (n) => n > 0,
  );
  return counts.map((n) => `repeat(${n}, ${pad})`).join(` ${gutter} `);
}
