/** Control hint model for the persistent hint bar.
 *
 * Retained FL Studio 25.1.1.4879 probe: the hint panel names the control under the pointer on
 * one line, right-aligns its keyboard shortcut and shows an optional live value readout
 * (docs/development/fl-compliance/rea-2026-10-20). Genos reads the same facts from the
 * existing `title` / `aria-label` attributes, so every control keeps one source of truth. */
export interface ControlHint {
  readonly name: string;
  readonly shortcut: string;
  readonly value: string;
}

const KEY_NAME =
  "(?:[A-Za-z0-9]|Space|Enter|Escape|Esc|Delete|Backspace|Tab|Home|End|PageUp|PageDown|Arrow(?:Up|Down|Left|Right)|F\\d{1,2})";
const SHORTCUT = new RegExp(`^(?:(?:Ctrl|Shift|Alt)\\+)*${KEY_NAME}$`);
const TRAILING_GROUP = /^(.*?)\s*\(([^()]*)\)\s*$/;

export const EMPTY_HINT: ControlHint = { name: "", shortcut: "", value: "" };

/** Split "Undo (Ctrl+Z)" into name and shortcut; descriptive parentheses stay in the name. */
export function parseHint(raw: string): Pick<ControlHint, "name" | "shortcut"> {
  const text = raw.trim();
  const match = TRAILING_GROUP.exec(text);
  if (match && match[1] && SHORTCUT.test(match[2].trim())) {
    return { name: match[1].trim(), shortcut: match[2].trim() };
  }
  return { name: text, shortcut: "" };
}

/** Hint for a control: title, else aria-label, plus an optional `data-hint-value` readout. */
export function hintForElement(el: Element | null): ControlHint {
  if (!el) return EMPTY_HINT;
  const raw =
    el.getAttribute("data-hint") ||
    el.getAttribute("title") ||
    el.getAttribute("aria-label") ||
    "";
  if (!raw.trim()) return EMPTY_HINT;
  return { ...parseHint(raw), value: el.getAttribute("data-hint-value") ?? "" };
}
