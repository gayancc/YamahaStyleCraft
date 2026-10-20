import type { KeyboardEvent } from "react";

/** The same focus order for modal drafts, utilities and disclosures. */
export function focusableControls(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'button,input:not([type="hidden"]),select,textarea,a[href],summary,[tabindex="0"]',
    ),
  ].filter((el) => {
    if (
      el.matches(":disabled") ||
      el.tabIndex < 0 ||
      el.closest("[hidden],[inert]")
    )
      return false;
    for (
      let parent: HTMLElement | null = el;
      parent && parent !== root.parentElement;
      parent = parent.parentElement
    ) {
      const style = getComputedStyle(parent);
      if (style.display === "none" || style.visibility === "hidden")
        return false;
      if (
        parent instanceof HTMLDetailsElement &&
        !parent.open &&
        !parent.querySelector("summary")?.contains(el)
      )
        return false;
    }
    return true;
  });
}

export function restoreEditorFocus(previous: HTMLElement | null) {
  if (!previous?.isConnected) return;
  const disclosure = previous.closest("details:not([open])");
  (disclosure?.querySelector("summary") ?? previous).focus();
}

export function cycleModalFocus(
  event: Pick<
    KeyboardEvent<HTMLElement>,
    "key" | "shiftKey" | "preventDefault"
  >,
  root: HTMLElement,
) {
  if (event.key !== "Tab") return;
  const controls = focusableControls(root);
  const first = controls[0],
    last = controls.at(-1);
  if (!first) {
    event.preventDefault();
    return;
  }
  if (
    event.shiftKey &&
    (document.activeElement === first ||
      !controls.includes(document.activeElement as HTMLElement))
  ) {
    event.preventDefault();
    last?.focus();
  } else if (
    !event.shiftKey &&
    (document.activeElement === last ||
      !controls.includes(document.activeElement as HTMLElement))
  ) {
    event.preventDefault();
    first.focus();
  }
}

export function trapModalFocus(event: KeyboardEvent<HTMLElement>) {
  // Escape belongs to the dialog's dismiss handler (often on its backdrop).
  if (event.key !== "Escape") event.stopPropagation();
  cycleModalFocus(event, event.currentTarget);
}
