import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useFloatingEditor } from "./FloatingToolWindow";
import type { Note, Section, View } from "./contracts";
import { useStudio } from "./store";
import { Music2, RotateCcw, Check, X } from "lucide-react";
import { ParameterControl } from "./ParameterControl";
import { PitchPicker } from "./PitchPicker";
import { meterAt } from "./pianoEditing";
import {
  formatNoteTime,
  formatNoteDuration,
  parseNoteDuration,
  pitchName,
  validateNoteProperties,
} from "./notePropertiesDraft";

export function NoteProperties({
  note,
  view,
  section,
  channel,
  position,
  disabled,
  onClose,
}: {
  note: Note;
  view: View;
  section: Section;
  channel: number;
  position: { x: number; y: number };
  disabled: boolean;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(() => ({
    time: formatNoteTime(note.start, view, section),
    pitch: String(note.pitch),
    velocity: String(note.velocity),
    duration: String(note.duration),
  }));
  const [error, setError] = useState(""),
    [applying, setApplying] = useState(false);
  const [durationText, setDurationText] = useState(() =>
    formatNoteDuration(note.duration, view, section),
  );
  const opened = useRef({ view, section, channel, note });
  const time = useRef<HTMLInputElement>(null);
  const { ref: dialog, handle } = useFloatingEditor(
    {
      x:
        position.x + 14 > innerWidth - 328 ? position.x - 334 : position.x + 14,
      y: position.y + 12,
    },
    true,
    applying,
  );
  useLayoutEffect(() => {
    time.current?.focus();
    time.current?.select();
  }, []);
  useEffect(() => {
    if (
      !applying &&
      (view !== opened.current.view ||
        section.id !== opened.current.section.id ||
        channel !== opened.current.channel)
    )
      onClose();
  }, [view, section.id, channel, applying, onClose]);
  const reset = () => {
    setDraft({
      time: formatNoteTime(note.start, view, section),
      pitch: String(note.pitch),
      velocity: String(note.velocity),
      duration: String(note.duration),
    });
    setDurationText(formatNoteDuration(note.duration, view, section));
    setError("");
  };
  const change = (field: keyof typeof draft, value: string) =>
    setDraft((old) => ({ ...old, [field]: value }));
  const submit = async () => {
    if (applying || disabled) return;
    setError("");
    const current = useStudio.getState();
    if (
      current.busy ||
      current.view !== opened.current.view ||
      current.sectionId !== section.id ||
      current.channel !== channel ||
      !current.view.notes.some((n) => n.id === note.id && n.channel === channel)
    ) {
      setError(
        "This note or its editing context changed. Close and reopen its properties.",
      );
      return;
    }
    try {
      const update = validateNoteProperties(
        {
          ...draft,
          duration: String(parseNoteDuration(durationText, view, section)),
        },
        note.id,
        view,
        section,
      );
      if (
        update.start === note.start &&
        update.duration === note.duration &&
        update.pitch === note.pitch &&
        update.velocity === note.velocity
      ) {
        onClose();
        return;
      }
      setApplying(true);
      await current.edit({
        type: "pianoNotes",
        section: section.id,
        channel,
        updates: [update],
        moveControllers: false,
      });
      const result = useStudio.getState();
      if (result.error) {
        setError(result.error);
        setApplying(false);
      } else onClose();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
      setApplying(false);
    }
  };
  return createPortal(
    <section
      ref={dialog}
      className="note-utility-window"
      role="dialog"
      aria-modal="false"
      aria-labelledby="note-properties-title"
      tabIndex={-1}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" && !applying) {
          e.preventDefault();
          onClose();
        }
      }}
    >
      <header className="note-utility-titlebar">
        <button
          type="button"
          className="note-utility-drag"
          aria-label="Move note properties"
          title="Drag to move · Arrow keys move this window"
          {...handle}
        >
          <Music2 size={13} />
          <span id="note-properties-title">Note properties</span>
          <strong>{pitchName(Number(draft.pitch))}</strong>
        </button>
        <button
          type="button"
          className="note-utility-close"
          aria-label="Close note properties"
          disabled={applying}
          onClick={onClose}
        >
          <X size={13} />
        </button>
      </header>
      <div className="note-utility-context">
        <span>{section.label}</span>
        <label>
          Source CH{" "}
          <input
            aria-label="Note properties source channel"
            readOnly
            tabIndex={-1}
            value={channel}
          />
        </label>
      </div>
      <form
        noValidate
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <fieldset
          disabled={disabled || applying}
          className="note-utility-controls"
        >
          <div className="note-utility-timing">
            <div className="note-utility-pitch">
              <span>Pitch</span>
              <PitchPicker
                value={Number(draft.pitch)}
                onChange={(p) => change("pitch", String(p))}
              />
            </div>
            <label className="note-utility-readout">
              <span>Start</span>
              <input
                ref={time}
                aria-label="Time (bar:beat:tick)"
                title="Bar : beat : tick · Starts at 1:1:0"
                spellCheck={false}
                value={draft.time}
                onChange={(e) => change("time", e.target.value)}
              />
            </label>
            <label className="note-utility-readout">
              <span>Length</span>
              <input
                aria-label="Note properties duration"
                title="Bars : beats : ticks · Length starts at 0:0:0"
                spellCheck={false}
                value={durationText}
                onChange={(e) => setDurationText(e.target.value)}
              />
            </label>
            <span className="note-utility-units">
              bar : beat : tick · {meterAt(view, section.start).beat} ticks/beat
            </span>
          </div>
          <div className="note-utility-level">
            <ParameterControl
              label="Velocity"
              ariaLabel="Note properties velocity"
              value={Number(draft.velocity)}
              min={1}
              resetValue={note.velocity}
              disabled={disabled || applying}
              onCommit={(v) => change("velocity", String(v))}
            />
          </div>
        </fieldset>
        {error && (
          <p className="note-utility-error" role="alert">
            {error}
          </p>
        )}
        <footer className="note-utility-actions">
          <button
            type="button"
            title="Restore the values from when this editor opened"
            disabled={disabled || applying}
            onClick={reset}
          >
            <RotateCcw size={12} />
            Reset
          </button>
          <span className="spacer" />
          <button type="button" disabled={applying} onClick={onClose}>
            Cancel
          </button>
          <button
            type="submit"
            className="note-utility-accept"
            disabled={disabled || applying}
          >
            <Check size={13} />
            Apply
          </button>
        </footer>
      </form>
    </section>,
    document.body,
  );
}
