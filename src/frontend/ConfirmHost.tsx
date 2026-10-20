import { useEffect, useRef } from "react";
import { usePendingConfirm } from "./confirmDialog";
import { restoreEditorFocus, trapModalFocus } from "./modal";

/** Renders the pending confirmation as an FL-style dialog: caption strip, message, footer strip.
 * The safe choice (Cancel) takes initial focus; Escape cancels. */
export function ConfirmHost() {
  const pending = usePendingConfirm();
  const dialog = useRef<HTMLElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!pending) return;
    const prior = document.activeElement as HTMLElement | null;
    cancel.current?.focus();
    return () => restoreEditorFocus(prior);
  }, [pending]);
  if (!pending) return null;
  const title = pending.title ?? "Confirm";
  return (
    <div
      className="modal-backdrop confirm-backdrop"
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") {
          e.preventDefault();
          pending.settle(false);
        }
      }}
    >
      <section
        ref={dialog}
        className="modal confirm-modal"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-message"
        tabIndex={-1}
        onKeyDown={trapModalFocus}
      >
        <div className="modal-title">
          <h2 id="confirm-title">{title}</h2>
        </div>
        <p id="confirm-message" className="confirm-message">
          {pending.message}
        </p>
        <div className="modal-actions">
          <button
            ref={cancel}
            type="button"
            onClick={() => pending.settle(false)}
          >
            {pending.cancelLabel ?? "Cancel"}
          </button>
          <button
            type="button"
            className="primary"
            onClick={() => pending.settle(true)}
          >
            {pending.confirmLabel ?? "OK"}
          </button>
        </div>
      </section>
    </div>
  );
}
