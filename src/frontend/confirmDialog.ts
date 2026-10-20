import { useSyncExternalStore } from "react";

export interface ConfirmOptions {
  title?: string;
  confirmLabel?: string;
  cancelLabel?: string;
}

export interface PendingConfirm extends ConfirmOptions {
  message: string;
  settle: (accepted: boolean) => void;
}

let queue: readonly PendingConfirm[] = [];
const listeners = new Set<() => void>();

function publish(next: readonly PendingConfirm[]) {
  queue = next;
  listeners.forEach((notify) => notify());
}

/** App confirmation (replaces the OS/WebView confirm box). Resolves true only on explicit accept;
 * overlapping requests are shown one after another. */
export function askConfirm(
  message: string,
  options: ConfirmOptions = {},
): Promise<boolean> {
  return new Promise((resolve) => {
    const entry: PendingConfirm = {
      ...options,
      message,
      settle: (accepted) => {
        publish(queue.filter((q) => q !== entry));
        resolve(accepted);
      },
    };
    publish([...queue, entry]);
  });
}

export function usePendingConfirm(): PendingConfirm | null {
  return useSyncExternalStore(
    (notify) => {
      listeners.add(notify);
      return () => void listeners.delete(notify);
    },
    () => queue[0] ?? null,
  );
}
