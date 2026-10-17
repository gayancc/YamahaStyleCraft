import type { KeyboardEvent } from "react";

export function trapModalFocus(event: KeyboardEvent<HTMLElement>) {
  event.stopPropagation();
  if (event.key !== "Tab") return;
  const controls = [
    ...event.currentTarget.querySelectorAll<HTMLElement>(
      'button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex="0"]',
    ),
  ];
  const first = controls[0],
    last = controls.at(-1);
  if (!first) {
    event.preventDefault();
    return;
  }
  if (
    event.shiftKey &&
    (document.activeElement === first ||
      document.activeElement === event.currentTarget)
  ) {
    event.preventDefault();
    last?.focus();
  } else if (
    !event.shiftKey &&
    (document.activeElement === last ||
      document.activeElement === event.currentTarget)
  ) {
    event.preventDefault();
    first.focus();
  }
}
