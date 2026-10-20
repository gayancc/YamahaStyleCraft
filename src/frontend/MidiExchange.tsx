import { restoreEditorFocus } from "./modal";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { ExchangePreview, Section, View } from "./contracts";
import { useStudio } from "./store";
import { pitchName } from "./PianoRoll";
import { trapModalFocus } from "./modal";

export function MidiExchange({
  section,
  channel,
  onClose,
}: {
  section: Section;
  channel: number;
  onClose: () => void;
}) {
  const { view, docLabel } = useStudio();
  const [original] = useState(view!);
  const [preview, setPreview] = useState<ExchangePreview | null>(null);
  const [file, setFile] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [wholeSection, setWholeSection] = useState(false);
  const working = useRef(false);
  const dialog = useRef<HTMLElement>(null);
  const stale = view !== original;
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    return () => restoreEditorFocus(prior);
  }, []);
  useEffect(() => {
    if (pending) dialog.current?.focus();
  }, [pending]);
  const run = async (action: () => Promise<void>) => {
    if (working.current || useStudio.getState().busy) return;
    working.current = true;
    setPending(true);
    setError(null);
    useStudio.setState({ busy: true });
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      working.current = false;
      setPending(false);
      useStudio.setState({ busy: false });
    }
  };
  const ensureCurrent = () => {
    if (useStudio.getState().view !== original)
      throw new Error("The document changed. Close and reopen MIDI exchange.");
  };
  const args = {
    section: section.id,
    channel,
    revision: original.revision,
    wholeSection,
    docLabel,
  };
  const exportMidi = () =>
    run(async () => {
      ensureCurrent();
      const scope = wholeSection
        ? `${section.label}-section`
        : `${section.label}-source-${channel}`;
      const name = `${original.name}-${scope}`.replace(/[<>:"/\\|?*]/g, "_");
      const path = await save({
        defaultPath: `${name}.mid`,
        filters: [{ name: "Standard MIDI", extensions: ["mid"] }],
      });
      if (!path) return;
      ensureCurrent();
      await invoke("export_phrase_midi", { path, ...args });
      useStudio.setState({
        message: wholeSection
          ? "Section sources exported as multi-track MIDI"
          : "Source phrase exported as MIDI",
      });
    });
  const chooseMidi = () =>
    run(async () => {
      ensureCurrent();
      setPreview(null);
      setFile("");
      const path = await open({
        multiple: false,
        filters: [{ name: "Standard MIDI", extensions: ["mid"] }],
      });
      if (typeof path !== "string") return;
      ensureCurrent();
      const result = await invoke<ExchangePreview>("preview_phrase_midi", {
        path,
        ...args,
      });
      ensureCurrent();
      setPreview(result);
      setFile(path);
    });
  const apply = () =>
    run(async () => {
      ensureCurrent();
      if (!preview) return;
      const next = await invoke<View>("apply_phrase_midi", {
        token: preview.token,
      });
      useStudio.setState({
        view: next,
        selected: [],
        message: preview.unchanged
          ? "MIDI notes unchanged"
          : "MIDI notes imported · one undo step",
      });
      close.current();
    });
  const dismiss = () => {
    if (!working.current) close.current();
  };
  return (
    <div
      className="modal-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
    >
      <section
        className="modal midi-exchange-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="exchange-title"
        ref={dialog}
        tabIndex={-1}
        onKeyDown={(e) => {
          trapModalFocus(e);
          if (e.key === "Escape") {
            e.stopPropagation();
            dismiss();
          }
        }}
      >
        <header className="modal-title">
          <h2 id="exchange-title">MIDI exchange</h2>
          <p className="exchange-scope">
            {section.label} · Source {channel}
          </p>
        </header>
        <div className="exchange-body">
          <p>
            Export this phrase, edit it in your DAW, then choose the returned
            MIDI to review its notes.
          </p>
          <label className="source-check">
            <input
              type="checkbox"
              checked={wholeSection}
              disabled={pending || stale}
              onChange={(e) => {
                setWholeSection(e.target.checked);
                setPreview(null);
                setFile("");
              }}
            />{" "}
            Entire section (all source channels)
          </label>
          <div className="exchange-actions">
            <button
              disabled={pending || stale}
              onClick={() => void exportMidi()}
            >
              {wholeSection ? "Export section MIDI…" : "Export phrase MIDI…"}
            </button>
            <button
              disabled={pending || stale}
              onClick={() => void chooseMidi()}
            >
              Choose edited MIDI…
            </button>
          </div>
          <p className="exchange-hint">
            {wholeSection
              ? "Type-1 multi-track MIDI, one track per source with notes. Up to 8 MiB. Import matches channels and replaces notes only; voices, controllers, tempo and CASM stay."
              : "One note channel, up to 8 MiB. Keep the phrase at its original tick offset. Import replaces only this source's notes in this section; it keeps the Style's voices, controllers, tempo and CASM."}
          </p>
          {preview && (
            <div className="exchange-preview" aria-label="MIDI import preview">
              <p className="exchange-path">{file}</p>
              <dl>
                <div>
                  <dt>Notes</dt>
                  <dd>
                    {preview.originalNotes} → {preview.importedNotes}
                  </dd>
                </div>
                <div>
                  <dt>Channel</dt>
                  <dd>
                    {preview.channels?.length
                      ? `${preview.channels.length} sources`
                      : `${preview.sourceChannel} → Source ${preview.targetChannel}`}
                  </dd>
                </div>
                {preview.channels && preview.channels.length > 0 && (
                  <div>
                    <dt>Per source</dt>
                    <dd>
                      {preview.channels
                        .map(
                          (c) =>
                            `CH${c.channel}: ${c.originalNotes}→${c.importedNotes}`,
                        )
                        .join(" · ")}
                    </dd>
                  </div>
                )}
                <div>
                  <dt>Resolution</dt>
                  <dd>
                    {preview.sourcePpq} → {preview.targetPpq} PPQ
                  </dd>
                </div>
                <div>
                  <dt>Phrase end</dt>
                  <dd>{preview.endTick} ticks from section start</dd>
                </div>
              </dl>
              <details className="exchange-note-preview">
                <summary>
                  Inspect {preview.notePreview.length} of{" "}
                  {preview.importedNotes} imported notes
                </summary>
                <div className="exchange-note-table">
                  <table>
                    <caption>
                      Ticks relative to section start at {preview.targetPpq} PPQ
                    </caption>
                    <thead>
                      <tr>
                        <th>Note</th>
                        <th>Start</th>
                        <th>Length</th>
                        <th>Velocity</th>
                      </tr>
                    </thead>
                    <tbody>
                      {preview.notePreview.map((note, index) => (
                        <tr key={index}>
                          <td>
                            {pitchName(note.pitch)} ({note.pitch})
                          </td>
                          <td>{note.start}</td>
                          <td>{note.duration}</td>
                          <td>{note.velocity}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </details>
              <p>
                {preview.unchanged
                  ? "Notes are unchanged. Applying keeps the original bytes and history."
                  : "Apply replaces the listed source notes as one undo step."}
              </p>
              <p>
                {preview.ignoredEvents} non-note events will be ignored. Notes
                outside the section are rejected, never clipped.
              </p>
            </div>
          )}
          {(error || stale) && (
            <p role="alert" className="source-error">
              {stale
                ? "The document changed. Close and reopen MIDI exchange."
                : error}
            </p>
          )}
        </div>
        <footer className="exchange-actions modal-actions">
          <button disabled={pending} onClick={dismiss}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={pending || stale || !preview}
            onClick={() => void apply()}
          >
            {pending ? "Working…" : "Apply MIDI notes"}
          </button>
        </footer>
      </section>
    </div>
  );
}
