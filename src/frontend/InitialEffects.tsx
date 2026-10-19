import { useEffect, useRef, useState } from "react";
import { CircleHelp } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type { Port, View } from "./contracts";

export type EffectSelection = {
  view: View;
  output: Port;
  token: string;
  packetCount: number;
  lateChanges: boolean;
};
export function InitialEffects({
  view,
  output,
  disabled,
  selection,
  onChange,
}: {
  view: View | null;
  output: Port | null;
  disabled: boolean;
  selection: EffectSelection | null;
  onChange: (value: EffectSelection | null) => void;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const current = useRef({ view, output });
  current.current = { view, output };
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const checked =
    !!selection && selection.view === view && selection.output === output;
  const toggle = async () => {
    if (pending || disabled) return;
    setError(null);
    if (checked) {
      onChange(null);
      return;
    }
    if (!view || !output) return;
    setPending(true);
    try {
      const preview = await invoke<{
        token: string;
        packets: number[][];
        header: {
          reverb: { lateChanges: boolean };
          chorus: { lateChanges: boolean };
        };
      }>("preview_style_effects", { revision: view.revision });
      if (
        mounted.current &&
        current.current.view === view &&
        current.current.output === output
      ) {
        onChange({
          view,
          output,
          token: preview.token,
          packetCount: preview.packets.length,
          lateChanges:
            preview.header.reverb.lateChanges ||
            preview.header.chorus.lateChanges,
        });
      }
    } catch (e) {
      if (
        mounted.current &&
        current.current.view === view &&
        current.current.output === output
      )
        setError(String(e));
    } finally {
      if (mounted.current) setPending(false);
    }
  };
  return (
    <div className="initial-effects">
      <div className="initial-effects-row">
        <label>
          <input
            type="checkbox"
            aria-label="Preview initial Genos effects"
            checked={checked}
            disabled={disabled || pending || !view || !output}
            onChange={() => void toggle()}
          />{" "}
          Preview initial Genos effects
        </label>
        <span
          className="help-tip"
          tabIndex={0}
          role="img"
          aria-label="Initial effects help"
          data-tip="Sends known reverb/chorus type and Return values once before Play. These are global keyboard settings. Detailed parameters, drum setup, and later effect changes are not applied. Selection resets after document or output changes."
        >
          <CircleHelp size={13} aria-hidden />
        </span>
      </div>
      {pending && (
        <p className="muted" role="status">
          Checking initial effect settings…
        </p>
      )}
      {checked && (
        <p className="muted" role="status">
          {selection.packetCount} typed messages ready for the next Play.
          {selection.lateChanges
            ? " Later effect messages exist and will not be replayed."
            : ""}
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
