import { useStudio } from "./store";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CatalogVoice, MonitorEvent } from "./contracts";
import { USER_VOICE_CATEGORY } from "./userVoices";

const LEARN_TIMEOUT_MS = 45000;

interface Draft {
  name: string;
  kit: boolean;
  msb: string;
  lsb: string;
  program: string;
}

const toDraft = (v: CatalogVoice | null): Draft => ({
  name: v?.name ?? "",
  kit: v?.voiceType === "UserKit",
  msb: String(v?.msb ?? 0),
  lsb: String(v?.lsb ?? 0),
  program: String((v?.program ?? 0) + 1),
});

const parse = (text: string, min: number, max: number) => {
  const n = Number(text);
  return text.trim() !== "" && Number.isInteger(n) && n >= min && n <= max
    ? n
    : null;
};

/** Add or edit one hand-entered voice / kit. "Learn" listens to the connected keyboard: pick the
 * voice on its panel and the Bank Select + Program Change it sends fill the address for you. */
export function UserVoiceEditor({
  initial,
  midiInput,
  monitoringActive,
  onSave,
  onCancel,
}: {
  initial: CatalogVoice | null;
  midiInput?: string;
  monitoringActive: boolean;
  onSave: (voice: CatalogVoice) => void;
  onCancel: () => void;
}) {
  const docLabel = useStudio((state) => state.docLabel);
  const [draft, setDraft] = useState(() => toDraft(initial));
  const [learning, setLearning] = useState(false);
  const [note, setNote] = useState("");
  const set = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));

  useEffect(() => {
    if (!learning || !midiInput) return;
    let live = true;
    const startedHere = !monitoringActive;
    let baseline = -1;
    let seenAtBaseline = 0;
    const banks = new Map<number, { msb?: number; lsb?: number }>();
    const finish = () => {
      if (live) setLearning(false);
    };
    const poll = async () => {
      const [, events] =
        await invoke<[string | null, MonitorEvent[]]>("monitor_snapshot", { docLabel });
      if (!live) return;
      if (!startedHere && baseline < 0) {
        baseline = events.reduce((m, e) => Math.max(m, e.stampMs), 0);
        seenAtBaseline = events.filter((e) => e.stampMs === baseline).length;
        return;
      }
      let atBaseline = 0;
      for (const e of events) {
        if (e.stampMs < baseline) continue;
        if (e.stampMs > baseline) {
          baseline = e.stampMs;
          seenAtBaseline = 0;
          atBaseline = 0;
        }
        if (++atBaseline <= seenAtBaseline) continue;
        seenAtBaseline = atBaseline;
        const [status, d1, d2] = e.data;
        const type = status & 0xf0;
        const channel = status & 15;
        const bank = banks.get(channel) ?? {};
        if (type === 0xb0 && e.data.length === 3 && d2 < 128) {
          if (d1 === 0) bank.msb = d2;
          else if (d1 === 32) bank.lsb = d2;
          else continue;
          banks.set(channel, bank);
        } else if (type === 0xc0 && e.data.length === 2 && d1 < 128) {
          const { msb, lsb } = bank;
          set({
            ...(msb !== undefined ? { msb: String(msb) } : {}),
            ...(lsb !== undefined ? { lsb: String(lsb) } : {}),
            program: String(d1 + 1),
          });
          setNote(
            msb === undefined || lsb === undefined
              ? "Program received with incomplete bank data. Check MSB/LSB."
              : "Address learned from the keyboard.",
          );
          finish();
          return;
        }
      }
    };
    let timer = 0;
    let polling = false;
    let monitorOpened = false;
    const releaseMonitor = () => {
      if (!monitorOpened) return;
      monitorOpened = false;
      void invoke("stop_monitor", { docLabel }).catch(() => undefined);
    };
    const begin = async () => {
      try {
        if (startedHere) {
          await invoke("start_monitor", { name: midiInput, docLabel });
          monitorOpened = true;
        }
        if (!live) {
          releaseMonitor();
          return;
        }
        await poll();
        if (!live) return;
        timer = window.setInterval(() => {
          if (polling) return;
          polling = true;
          void poll().catch(() => undefined).finally(() => { polling = false; });
        }, 250);
      } catch (err) {
        if (!live) return;
        setNote(String(err));
        finish();
      }
    };
    void begin();
    const timeout = window.setTimeout(() => {
      setNote(
        "Nothing received. Check the MIDI input and that the keyboard transmits.",
      );
      finish();
    }, LEARN_TIMEOUT_MS);
    return () => {
      live = false;
      window.clearInterval(timer);
      window.clearTimeout(timeout);
      releaseMonitor();
    };
  }, [learning, midiInput, monitoringActive, docLabel]);

  const msb = parse(draft.msb, 0, 127);
  const lsb = parse(draft.lsb, 0, 127);
  const program = parse(draft.program, 1, 128);
  const valid =
    draft.name.trim() !== "" &&
    msb !== null &&
    lsb !== null &&
    program !== null;
  const save = () => {
    if (!valid) return;
    onSave({
      category: USER_VOICE_CATEGORY,
      subcategory: "-",
      name: draft.name.trim(),
      msb,
      lsb,
      program: program - 1,
      voiceType: draft.kit ? "UserKit" : "UserVoice",
      page: 0,
    });
  };

  return (
    <form
      className="user-voice-editor"
      aria-label={initial ? "Edit my voice" : "Add my voice"}
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
      onKeyDown={(e) => e.stopPropagation()}
    >
      <label className="uv-name">
        <span>Name</span>
        <input
          aria-label="Voice name"
          autoFocus
          maxLength={60}
          placeholder="e.g. Expansion Slap Bass"
          value={draft.name}
          onChange={(e) => set({ name: e.target.value })}
        />
      </label>
      <div className="uv-address">
        {(
          [
            ["MSB", "msb"],
            ["LSB", "lsb"],
            ["Program", "program"],
          ] as const
        ).map(([label, key]) => (
          <label key={key}>
            <span>{label}</span>
            <input
              aria-label={`Voice ${label}`}
              inputMode="numeric"
              value={draft[key]}
              onChange={(e) => set({ [key]: e.target.value } as Partial<Draft>)}
            />
          </label>
        ))}
        <label className="uv-kit">
          <input
            type="checkbox"
            checked={draft.kit}
            onChange={(e) => set({ kit: e.target.checked })}
          />
          <span>Drum / SFX kit</span>
        </label>
      </div>
      <div className="uv-actions">
        <button
          type="button"
          disabled={!midiInput || learning}
          title={
            midiInput
              ? "Select the voice on your keyboard; its Bank Select + Program Change fill the address"
              : "Choose a MIDI input in MIDI connections first"
          }
          onClick={() => {
            setNote("Listening… pick the voice on your keyboard.");
            setLearning(true);
          }}
        >
          {learning ? "Listening…" : "Learn from keyboard"}
        </button>
        <span className="spacer" />
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="primary" disabled={!valid}>
          Save voice
        </button>
      </div>
      {note && (
        <p className="muted" role="status">
          {note}
        </p>
      )}
    </form>
  );
}
