import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent } from "react";
import { useEffect, useState } from "react";

const INTERACTIVE = "button,input,select,textarea,a,summary,[role='menu'],[role='menuitem']";

/** Tracks the native maximize state; undecorated windows must draw their own controls. */
function useWindowChrome() {
  const [decorated, setDecorated] = useState(true);
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!isTauri()) return;
    let off: (() => void) | undefined;
    let alive = true;
    try {
      const win = getCurrentWindow();
      const sync = () => {
        void win.isMaximized().then((m) => alive && setMaximized(m));
      };
      void win.isDecorated().then((d) => alive && setDecorated(d));
      sync();
      void win.onResized(sync).then((f) => (alive ? (off = f) : f()));
    } catch {
      // Window API unavailable (embedded test hosts): keep the native frame assumption.
    }
    return () => {
      alive = false;
      off?.();
    };
  }, []);
  return { decorated, maximized };
}

/** Drag the window by the empty title bar; double-click toggles maximize (Windows convention). */
export function titlebarDragProps() {
  const onNative = (e: MouseEvent) =>
    isTauri() && !(e.target as HTMLElement).closest(INTERACTIVE);
  return {
    onMouseDown: (e: MouseEvent) => {
      if (e.button === 0 && e.detail === 1 && onNative(e)) {
        void getCurrentWindow().startDragging();
      }
    },
    onDoubleClick: (e: MouseEvent) => {
      if (onNative(e)) void getCurrentWindow().toggleMaximize();
    },
  };
}

/** FL Studio's three window buttons: small beveled squares, minimize / maximize-restore / close. */
export function WindowControls() {
  const { decorated, maximized } = useWindowChrome();
  if (!isTauri() || decorated) return null;
  const win = getCurrentWindow();
  return (
    <div className="window-controls" role="group" aria-label="Window controls">
      <button
        type="button"
        className="wc-btn wc-min"
        aria-label="Minimize"
        data-hint="Minimize"
        onClick={() => void win.minimize()}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2.5 9.5h7" />
        </svg>
      </button>
      <button
        type="button"
        className="wc-btn wc-max"
        aria-label={maximized ? "Restore" : "Maximize"}
        data-hint={maximized ? "Restore" : "Maximize"}
        onClick={() => void win.toggleMaximize()}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true">
          {maximized ? (
            <path d="M4.5 2.5h5v5M2.5 4.5h5v5h-5z" />
          ) : (
            <path d="M2.5 2.5h7v7h-7z" />
          )}
        </svg>
      </button>
      <button
        type="button"
        className="wc-btn wc-close"
        aria-label="Close window"
        data-hint="Close"
        onClick={() => void win.close()}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" />
        </svg>
      </button>
    </div>
  );
}
