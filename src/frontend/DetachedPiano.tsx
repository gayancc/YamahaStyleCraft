import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Prefs, View } from "./contracts";
import { PianoRoll } from "./PianoRoll";
import { DEFAULT_NOTE_COLORS, sanitizeNoteColors } from "./noteColors";
import { tickFromElapsed } from "./pianoEditing";
import { useStudio } from "./store";
import { listenViewSync, type ViewSyncPayload } from "./detach";

/**
 * Secondary-window Piano Roll (SFM-068). Shares the Rust session; syncs View
 * via events from the main window and session_view refresh.
 */
export function DetachedPiano() {
  const state = useStudio();
  const { view, sectionId, channel } = state;
  const [error, setError] = useState<string | null>(null);
  const [playhead, setPlayhead] = useState<number | null>(null);
  const [noteColors, setNoteColors] = useState<string[]>(() => [
    ...DEFAULT_NOTE_COLORS,
  ]);
  const section = view?.sections.find((s) => s.id === sectionId) ?? null;

  const applySync = (p: ViewSyncPayload) => {
    const mine = useStudio.getState().docLabel;
    if (p.docLabel && p.docLabel !== mine) return;
    useStudio.setState({
      view: p.view,
      sectionId: p.sectionId,
      channel: p.channel,
      selected: p.selected,
      message: p.view ? "Synced from editor window" : "No Style open",
    });
  };

  useEffect(() => {
    void invoke<Prefs>("load_prefs")
      .then((p) => setNoteColors(sanitizeNoteColors(p.noteColors)))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    let live = true;
    const docLabel = useStudio.getState().docLabel;
    void invoke<View>("session_view", { docLabel })
      .then((v) => {
        if (!live) return;
        useStudio.setState({
          view: v,
          sectionId:
            useStudio.getState().sectionId ||
            v.sections.find((s) => s.label === "Main A")?.id ||
            v.sections[0]?.id ||
            0,
          message:
            docLabel === "main"
              ? "Detached Piano Roll · editing shared Style session"
              : `Detached Piano Roll · ${docLabel}`,
        });
        setError(null);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    let unlisten: (() => void) | undefined;
    void listenViewSync((p) => {
      if (live) {
        applySync(p);
        setError(null);
      }
    }).then((u) => {
      unlisten = u;
    });
    const title = getCurrentWebviewWindow();
    void title.setTitle(
      docLabel === "main"
        ? "Piano Roll · Genos Style Studio"
        : `Piano Roll · ${docLabel}`,
    );
    return () => {
      live = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!view || !section) {
      setPlayhead(null);
      return;
    }
    let live = true;
    const poll = () => {
      void invoke<{ playing: boolean; elapsedMs: number }>("transport_status")
        .then((t) => {
          if (!live) return;
          setPlayhead(
            t.playing ? tickFromElapsed(view, section, t.elapsedMs) : null,
          );
        })
        .catch(() => undefined);
    };
    poll();
    const id = window.setInterval(poll, 50);
    return () => {
      live = false;
      window.clearInterval(id);
    };
  }, [view, section]);

  if (error && !view) {
    return (
      <div className="detached-piano empty">
        <p role="alert">{error}</p>
        <p className="muted">Open a Style in the main window, then detach again.</p>
      </div>
    );
  }
  if (!view || !section) {
    return (
      <div className="detached-piano empty">
        <p className="muted">Waiting for Style session…</p>
      </div>
    );
  }

  return (
    <div className="detached-piano">
      <header className="detached-piano-bar">
        <strong>
          {section.label} / CH{channel}
        </strong>
        <span className="muted">{view.name}</span>
        <label>
          Section{" "}
          <select
            aria-label="Detached section"
            value={sectionId}
            onChange={(e) => state.section(Number(e.target.value))}
          >
            {view.sections.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Channel{" "}
          <select
            aria-label="Detached channel"
            value={channel}
            onChange={(e) => state.setChannel(Number(e.target.value))}
          >
            {Array.from({ length: 16 }, (_, i) => i + 1).map((ch) => (
              <option key={ch} value={ch}>
                CH{ch}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="ghost"
          onClick={() => void getCurrentWebviewWindow().close()}
        >
          Close
        </button>
      </header>
      <PianoRoll
        playheadTick={playhead}
        noteColors={noteColors}
        onNoteColorsChange={(next) => {
          setNoteColors(next);
          void invoke<Prefs>("load_prefs")
            .then((current) =>
              invoke("save_prefs", {
                prefs: { ...current, noteColors: next },
              }),
            )
            .catch(() => undefined);
        }}
      />
    </div>
  );
}
