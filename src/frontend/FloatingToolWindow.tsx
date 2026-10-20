import { useLayoutEffect, useRef } from "react";
import type { HTMLAttributes, ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { focusableControls, restoreEditorFocus } from "./modal";

/** Floating editors live above layout clipping; their position is measured in viewport pixels. */
export function useFloatingEditor(
  position: { x: number; y: number },
  open = true,
  locked = false,
) {
  const ref = useRef<HTMLElement>(null),
    location = useRef(position);
  const drag = useRef<{
    id: number;
    x: number;
    y: number;
    left: number;
    top: number;
  } | null>(null);
  const place = (x = location.current.x, y = location.current.y) => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    location.current = {
      x: Math.max(8, Math.min(x, innerWidth - r.width - 8)),
      y: Math.max(8, Math.min(y, innerHeight - r.height - 8)),
    };
    el.style.left = `${location.current.x}px`;
    el.style.top = `${location.current.y}px`;
  };
  useLayoutEffect(() => {
    if (!open || !ref.current) return;
    place();
    const el = ref.current;
    const front = () => {
      // Keep every utility below modal drafts, with the active utility on top.
      const windows = [
        ...document.querySelectorAll<HTMLElement>(
          ".floating-tool-window:not([hidden]),.note-utility-window",
        ),
      ]
        .filter((window) => window !== el)
        .sort((a, b) => Number(a.style.zIndex) - Number(b.style.zIndex));
      [...windows, el].forEach((window, i) => {
        window.style.zIndex = String(60 + i);
      });
    };
    const previous = document.activeElement as HTMLElement | null;
    front();
    el.addEventListener("pointerdown", front);
    el.addEventListener("focusin", front);
    const resize = () => place();
    const observer = new ResizeObserver(resize);
    observer.observe(ref.current);
    window.addEventListener("resize", resize);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", resize);
      el.removeEventListener("pointerdown", front);
      el.removeEventListener("focusin", front);
      queueMicrotask(() => {
        // React restores selection during commit, after layout cleanup. Return
        // focus once that finishes, unless another editor has already claimed it.
        if (
          (!el.isConnected || el.hidden) &&
          (el.contains(document.activeElement) ||
            document.activeElement === document.body)
        )
          restoreEditorFocus(previous);
      });
      drag.current = null;
    };
  }, [open]);
  const handle: HTMLAttributes<HTMLButtonElement> = {
    onPointerDown: (e) => {
      if (e.button !== 0 || locked) return;
      e.preventDefault();
      e.currentTarget.focus();
      e.currentTarget.setPointerCapture(e.pointerId);
      drag.current = {
        id: e.pointerId,
        x: e.clientX,
        y: e.clientY,
        left: location.current.x,
        top: location.current.y,
      };
    },
    onPointerMove: (e) => {
      const d = drag.current;
      if (d) place(d.left + e.clientX - d.x, d.top + e.clientY - d.y);
    },
    onPointerUp: (e) => {
      drag.current = null;
      if (e.currentTarget.hasPointerCapture(e.pointerId))
        e.currentTarget.releasePointerCapture(e.pointerId);
    },
    onPointerCancel: () => {
      drag.current = null;
    },
    onLostPointerCapture: () => {
      drag.current = null;
    },
    onKeyDown: (e) => {
      if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) {
        e.preventDefault();
        e.stopPropagation();
        place(
          location.current.x +
            (e.key === "ArrowLeft" ? -10 : e.key === "ArrowRight" ? 10 : 0),
          location.current.y +
            (e.key === "ArrowUp" ? -10 : e.key === "ArrowDown" ? 10 : 0),
        );
      }
    },
  };
  return { ref, handle };
}

export function FloatingToolWindow({
  open,
  title,
  context,
  onClose,
  children,
  className = "",
  closeLabel = `Close ${title}`,
}: {
  open: boolean;
  title: string;
  context: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
  closeLabel?: string;
}) {
  const { ref, handle } = useFloatingEditor(
    { x: Math.max(8, (innerWidth - 460) / 2), y: 80 },
    open,
  );
  useLayoutEffect(() => {
    if (!open || !ref.current) return;
    const controls = focusableControls(
      ref.current.querySelector<HTMLElement>(".floating-tool-content")!,
    );
    (
      controls.find((el) => el.matches("input,select,textarea")) ?? controls[0]
    )?.focus();
  }, [open]);
  return createPortal(
    <section
      ref={ref}
      className={`floating-tool-window ${className}`}
      hidden={!open}
      role="dialog"
      aria-modal="false"
      aria-label={title}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") {
          e.preventDefault();
          onClose();
        }
      }}
    >
      <header className="note-utility-titlebar">
        <button
          type="button"
          className="note-utility-drag"
          aria-label={`Move ${title}`}
          title="Drag to move · Arrow keys move this window"
          {...handle}
        >
          <span role="heading" aria-level={2}>
            {title}
          </span>
        </button>
        <button
          type="button"
          className="note-utility-close"
          aria-label={closeLabel}
          onClick={onClose}
        >
          <X size={13} />
        </button>
      </header>
      <div className="note-utility-context">{context}</div>
      <div className="floating-tool-content">{children}</div>
    </section>,
    document.body,
  );
}
