import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { MidiEvent, View } from "./contracts";
import { useStudio } from "./store";
import {
  broadcastMixerSync,
  listenMixerSync,
  listenViewSync,
  type MixerSyncPayload,
  type ViewSyncPayload,
} from "./detach";
import {
  MixerPanel,
  eqValueAt as readMixerEq,
  mixerPlayheadTick,
  pitchBendRangeSemitones,
} from "./MixerPanel";

/**
 * Secondary-window Mixer (SFM-084). Shares the Rust session; mute/solo/arm
 * sync with the main window via mixer events.
 */
export function DetachedMixer() {
  const state = useStudio();
  const { view, sectionId, busy } = state;
  const [error, setError] = useState<string | null>(null);
  const [muted, setMuted] = useState<number[]>([]);
  const [solo, setSolo] = useState<number[]>([]);
  const [armedChannel, setArmedChannel] = useState<number | null>(null);
  const [mixerApplyAll, setMixerApplyAll] = useState(false);
  const [playhead, setPlayhead] = useState<number | null>(null);

  const sources = useMemo(() => {
    if (!view) return [9, 10, 11, 12, 13, 14, 15, 16];
    const set = new Set(view.notes.map((n) => n.channel));
    for (const e of view.events) {
      // Channel is encoded in the lower nibble of the status byte.
      // Skip system messages (0xF0+) which have no channel.
      if (e.data[0] < 0xf0) set.add((e.data[0] & 0x0f) + 1);
    }
    const list = [...set].filter((c) => c >= 1 && c <= 16).sort((a, b) => a - b);
    return list.length ? list : [9, 10, 11, 12, 13, 14, 15, 16];
  }, [view]);

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

  const emitMixer = (next: Partial<MixerSyncPayload>) => {
    void broadcastMixerSync({
      docLabel: useStudio.getState().docLabel,
      muted,
      solo,
      armedChannel,
      mixerApplyAll,
      ...next,
    });
  };

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
              ? "Detached Mixer · editing shared Style session"
              : `Detached Mixer · ${docLabel}`,
        });
        setError(null);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    let unlistenView: (() => void) | undefined;
    let unlistenMixer: (() => void) | undefined;
    void listenViewSync((p) => {
      if (live) {
        applySync(p);
        setError(null);
      }
    }).then((u) => {
      unlistenView = u;
    });
    void listenMixerSync((p: MixerSyncPayload) => {
      if (!live) return;
      if (p.docLabel && p.docLabel !== docLabel) return;
      setMuted(p.muted);
      setSolo(p.solo);
      setArmedChannel(p.armedChannel);
      setMixerApplyAll(p.mixerApplyAll);
    }).then((u) => {
      unlistenMixer = u;
    });
    void getCurrentWebviewWindow().setTitle(
      docLabel === "main"
        ? "Mixer · Genos Style Studio"
        : `Mixer · ${docLabel}`,
    );
    return () => {
      live = false;
      unlistenView?.();
      unlistenMixer?.();
    };
  }, []);

  useEffect(() => {
    if (!view) {
      setPlayhead(null);
      return;
    }
    let live = true;
    const poll = () => {
      void invoke<{ playing: boolean; elapsedMs: number }>("transport_status")
        .then((t) => {
          if (!live) return;
          setPlayhead(
            mixerPlayheadTick(view, sectionId, t.playing, t.elapsedMs),
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
  }, [view, sectionId]);

  const valueAt = (
    channel: number,
    status: number,
    controller: number,
    fallback: number,
  ) => {
    if (!view) return fallback;
    const section = view.sections.find((s) => s.id === sectionId);
    if (!section) return fallback;
    const hit = [...view.events]
      .reverse()
      .find(
        (e: MidiEvent) =>
          e.tick >= section.start &&
          e.tick < section.end &&
          e.data[0] === (status | (channel - 1)) &&
          e.data[1] === controller,
      );
    return hit ? hit.data[2] : fallback;
  };

  const onCc = (channel: number, controller: number, value: number) => {
    if (!view) return;
    const section = view.sections.find((s) => s.id === sectionId);
    if (!section) return;
    if (mixerApplyAll) {
      void state.edit({
        type: "mixerCc",
        section: section.id,
        channel,
        controller,
        value,
        allSections: true,
      });
      return;
    }
    const existing = [...view.events]
      .reverse()
      .find(
        (e) =>
          e.tick >= section.start &&
          e.tick < section.end &&
          e.data[0] === (0xb0 | (channel - 1)) &&
          e.data[1] === controller,
      );
    if (existing) {
      void state.edit({
        type: "setEvent",
        id: existing.id,
        tick: existing.tick,
        data: [0xb0 | (channel - 1), controller, value],
      });
    } else {
      const track =
        view.notes.find((n) => n.channel === channel)?.track ?? 0;
      void state.edit({
        type: "addEvent",
        section: section.id,
        track,
        tick: section.start,
        data: [0xb0 | (channel - 1), controller, value],
      });
    }
  };

  const eqValueAt = (channel: number, band: "eqLow" | "eqHigh") =>
    readMixerEq(view, sectionId, channel, band);

  const onEq = (channel: number, band: "eqLow" | "eqHigh", value: number) => {
    if (!view) return;
    const section = view.sections.find((s) => s.id === sectionId);
    if (!section) return;
    void state.edit({
      type: "mixerEq",
      section: section.id,
      channel,
      band,
      value,
      allSections: mixerApplyAll,
    });
  };

  if (error && !view) {
    return (
      <div className="detached-piano empty">
        <p role="alert">{error}</p>
        <p className="muted">Open a Style in the main window, then detach again.</p>
      </div>
    );
  }
  if (!view) {
    return (
      <div className="detached-piano empty">
        <p className="muted">Waiting for Style session…</p>
      </div>
    );
  }

  return (
    <div className="detached-piano detached-mixer">
      <header className="detached-piano-bar">
        <strong>Mixer</strong>
        <span className="muted">{view.name}</span>
        <label>
          Section{" "}
          <select
            aria-label="Detached mixer section"
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
        <button
          type="button"
          className="ghost"
          onClick={() => void getCurrentWebviewWindow().close()}
        >
          Close
        </button>
      </header>
      <MixerPanel
        view={view}
        sectionId={sectionId}
        sources={sources}
        muted={muted}
        solo={solo}
        armedChannel={armedChannel}
        mixerApplyAll={mixerApplyAll}
        editingDisabled={busy}
        capturing={false}
        busy={busy}
        playheadTick={playhead}
        valueAt={valueAt}
        eqValueAt={eqValueAt}
        onCc={onCc}
        onEq={onEq}
        targetChannel={state.channel}
        onAutoPan={(ch, points, removeIds) => {
          void state.edit({
            type: "controllerPoints",
            section: sectionId,
            channel: ch,
            controller: 10,
            points,
            remove: removeIds,
          });
        }}
        onSafeFx={(ch, changes) => {
          void state.edit({
            type: "mixerCcBatch",
            section: sectionId,
            channel: ch,
            changes,
            allSections: mixerApplyAll,
          });
        }}
        pitchBendRangeAt={(ch) => {
          const section = view.sections.find((s) => s.id === sectionId);
          return section
            ? pitchBendRangeSemitones(view.events, ch, section.start)
            : null;
        }}
        onPitchBendRange={(ch, semitones) => {
          void state.edit({
            type: "pitchBendRange",
            section: sectionId,
            channel: ch,
            semitones,
            allSections: mixerApplyAll,
          });
        }}
        onMuteToggle={(ch) => {
          setMuted((a) => {
            const next = a.includes(ch) ? a.filter((x) => x !== ch) : [...a, ch];
            emitMixer({ muted: next });
            return next;
          });
        }}
        onSoloToggle={(ch) => {
          setSolo((a) => {
            const next = a.includes(ch) ? a.filter((x) => x !== ch) : [...a, ch];
            emitMixer({ solo: next });
            return next;
          });
        }}
        onArmToggle={(ch) => {
          setArmedChannel((a) => {
            const next = a === ch ? null : ch;
            emitMixer({ armedChannel: next });
            return next;
          });
        }}
        onMixerApplyAll={(next) => {
          setMixerApplyAll(next);
          emitMixer({ mixerApplyAll: next });
        }}
      />
    </div>
  );
}
