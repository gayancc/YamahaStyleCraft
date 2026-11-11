import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

/** The hint panel is the only place a hovered control reports its name and value (no tooltip bubbles). */
export interface HintState {
  readonly name: string;
  readonly value: string;
}

interface HintApi {
  readonly hint: HintState;
  /** `owner` lets a control clear only its own message, so moving between controls never wipes the new hint. */
  show: (owner: object, name: string, value?: string) => void;
  clear: (owner: object) => void;
}

const EMPTY: HintState = { name: "", value: "" };
const HintContext = createContext<HintApi>({ hint: EMPTY, show: () => {}, clear: () => {} });

export function HintProvider({ children }: { children: ReactNode }) {
  const [hint, setHint] = useState<HintState>(EMPTY);
  const owner = useRef<object | null>(null);
  const show = useCallback((who: object, name: string, value = "") => {
    owner.current = who;
    setHint({ name, value });
  }, []);
  const clear = useCallback((who: object) => {
    if (owner.current === who) {
      owner.current = null;
      setHint(EMPTY);
    }
  }, []);
  const api = useMemo(() => ({ hint, show, clear }), [hint, show, clear]);
  return <HintContext.Provider value={api}>{children}</HintContext.Provider>;
}

export const useHint = (): HintApi => useContext(HintContext);

/** `Name: value`, or just `Name` when there is no value. */
export const formatHint = (h: HintState): string =>
  h.value.length > 0 ? `${h.name}: ${h.value}` : h.name;

/** Two-line panel: dim context line, then the live hint. */
export function HintPanel({ context = "" }: { context?: string }) {
  const { hint } = useHint();
  return (
    <div className="fl-hint" role="status" aria-live="off">
      <div className="fl-hint__context">{context}</div>
      <div className="fl-hint__text" data-testid="hint-text">
        {formatHint(hint)}
      </div>
    </div>
  );
}
