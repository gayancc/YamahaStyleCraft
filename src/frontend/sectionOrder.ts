/** Yamaha-style section display order (SFM-020–024). */
export function sectionSortKey(label: string): [number, string] {
  const n = label.trim().toUpperCase();
  if (n.startsWith("INTRO")) return [0, n];
  if (n.startsWith("MAIN")) return [1, n];
  if (n.startsWith("FILL")) return [2, n];
  if (n === "BREAK" || n.startsWith("BREAK ")) return [3, n];
  if (n.startsWith("ENDING") || n.startsWith("END ")) return [4, n];
  return [5, n];
}

export function sortSections<T extends { label: string }>(sections: T[]): T[] {
  return [...sections].sort((a, b) => {
    const [ra, ka] = sectionSortKey(a.label);
    const [rb, kb] = sectionSortKey(b.label);
    return ra - rb || ka.localeCompare(kb);
  });
}
