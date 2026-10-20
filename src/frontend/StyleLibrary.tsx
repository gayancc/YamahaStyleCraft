import { restoreEditorFocus } from "./modal";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Prefs, Section, View } from "./contracts";
import { trapModalFocus } from "./modal";
import { useStudio } from "./store";

interface LibrarySection {
  id: number;
  label: string;
  bars: number;
  noteCount: number;
  channels: number[];
}
interface LibraryStyle {
  path: string;
  name: string;
  format: string;
  ppq: number;
  sections: LibrarySection[];
}

type LibraryTab = "files" | "patterns";
type PatternType = "all" | "intro" | "main" | "fill" | "break" | "ending";

interface PatternRow {
  key: string;
  style: LibraryStyle;
  section: LibrarySection;
  type: Exclude<PatternType, "all"> | "other";
}

function patternTypeOf(label: string): PatternRow["type"] {
  const t = label.trim().toLowerCase();
  if (t.startsWith("intro")) return "intro";
  if (t.startsWith("main")) return "main";
  if (t.startsWith("fill")) return "fill";
  if (t.startsWith("break")) return "break";
  if (t.startsWith("ending") || t.startsWith("end ")) return "ending";
  return "other";
}

export function StyleLibrary({
  view,
  destination,
  channel,
  output,
  onClose,
}: {
  view: View;
  destination: Section;
  channel: number;
  /** MIDI output for library preview audition (SFM-LIB-PREVIEW). */
  output: { index: number; name: string } | null;
  onClose: () => void;
}) {
  const { busy, docLabel } = useStudio();
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const [catalog, setCatalog] = useState<LibraryStyle[]>([]);
  const [lib, setLib] = useState<LibraryStyle | null>(null);
  const [sectionId, setSectionId] = useState(0);
  const [sourceChannel, setSourceChannel] = useState(channel);
  const [fit, setFit] = useState<"keep" | "fit" | "repeat">("fit");
  const [search, setSearch] = useState("");
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const [favorites, setFavorites] = useState<string[]>([]);
  const [tab, setTab] = useState<LibraryTab>("files");
  const [patternType, setPatternType] = useState<PatternType>("all");
  const [includeVoice, setIncludeVoice] = useState(true);
  const [includeControllers, setIncludeControllers] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const section = lib?.sections.find((s) => s.id === sectionId);

  const filteredStyles = useMemo(() => {
    return catalog.filter((s) => {
      if (favoritesOnly && !favorites.includes(s.path)) return false;
      if (!search.trim()) return true;
      return s.name.toLowerCase().includes(search.trim().toLowerCase());
    });
  }, [catalog, favorites, favoritesOnly, search]);

  const patterns = useMemo(() => {
    const q = search.trim().toLowerCase();
    const rows: PatternRow[] = [];
    for (const style of catalog) {
      if (favoritesOnly && !favorites.includes(style.path)) continue;
      for (const sec of style.sections) {
        if (sec.noteCount <= 0) continue;
        const type = patternTypeOf(sec.label);
        if (patternType !== "all" && type !== patternType) continue;
        if (q && !`${style.name} ${sec.label}`.toLowerCase().includes(q)) {
          continue;
        }
        rows.push({
          key: `${style.path}::${sec.id}`,
          style,
          section: sec,
          type,
        });
      }
    }
    return rows;
  }, [catalog, favorites, favoritesOnly, patternType, search]);

  useEffect(() => {
    void invoke<Prefs>("load_prefs")
      .then((p) => setFavorites(p.libraryFavorites ?? []))
      .catch(() => undefined);
  }, []);

  const persistFavorites = async (next: string[]) => {
    setFavorites(next);
    try {
      const current = await invoke<Prefs>("load_prefs");
      await invoke("save_prefs", {
        prefs: { ...current, libraryFavorites: next },
      });
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleFavorite = (path: string) => {
    const next = favorites.includes(path)
      ? favorites.filter((p) => p !== path)
      : [...favorites, path].slice(0, 64);
    void persistFavorites(next);
  };

  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    return () => restoreEditorFocus(prior);
  }, []);

  const dismiss = () => {
    if (!pending && !busy) close.current();
  };

  const selectStyle = (next: LibraryStyle, preferSection?: number) => {
    setLib(next);
    const first =
      (preferSection != null
        ? next.sections.find((s) => s.id === preferSection)
        : undefined) ??
      next.sections.find((s) => s.noteCount > 0) ??
      next.sections[0];
    setSectionId(first?.id ?? 0);
    setSourceChannel(first?.channels[0] ?? channel);
  };

  const selectPattern = (row: PatternRow) => {
    selectStyle(row.style, row.section.id);
  };

  const chooseFile = async () => {
    setError(null);
    const path = await open({
      multiple: false,
      filters: [{ name: "Yamaha Style", extensions: ["sty", "STY"] }],
    });
    if (typeof path !== "string") return;
    setPending(true);
    try {
      const next = await invoke<LibraryStyle>("inspect_library_style", { path });
      setCatalog([next]);
      selectStyle(next);
      setTab("files");
    } catch (e) {
      setError(String(e));
      setLib(null);
      setCatalog([]);
    } finally {
      setPending(false);
    }
  };

  const chooseFolder = async () => {
    setError(null);
    const path = await open({ directory: true, multiple: false });
    if (typeof path !== "string") return;
    setPending(true);
    try {
      const next = await invoke<LibraryStyle[]>("scan_library_folder", { path });
      setCatalog(next);
      if (next[0]) selectStyle(next[0]);
      else {
        setLib(null);
        setError("No readable .STY files in that folder (max 200 scanned).");
      }
    } catch (e) {
      setError(String(e));
      setCatalog([]);
      setLib(null);
    } finally {
      setPending(false);
    }
  };

  const importPhrase = async () => {
    if (!lib || !section) return;
    setPending(true);
    setError(null);
    try {
      const next = await invoke<View>("import_library_phrase", {
        path: lib.path,
        librarySection: section.id,
        libraryChannel: sourceChannel,
        destinationSection: destination.id,
        destinationChannel: channel,
        fit,
        includeVoice,
        includeControllers,
        revision: view.revision,
        docLabel,
      });
      const extras = [
        includeVoice ? "voice" : null,
        includeControllers ? "CC" : null,
      ]
        .filter(Boolean)
        .join("+");
      useStudio.setState({
        view: next,
        selected: [],
        message: `Imported ${lib.name} · ${section.label} CH${sourceChannel} → ${destination.label} CH${channel}${extras ? ` (+${extras})` : ""}`,
      });
      close.current();
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  };

  const importCompletePart = async (drumsOnly = false) => {
    if (!lib || !section) return;
    setPending(true);
    setError(null);
    try {
      const next = await invoke<View>("import_library_section", {
        path: lib.path,
        librarySection: section.id,
        destinationSection: destination.id,
        fit,
        revision: view.revision,
        drumsOnly,
        includeVoice: drumsOnly ? false : includeVoice,
        includeControllers: drumsOnly ? false : includeControllers,
        docLabel,
      });
      const extras = drumsOnly
        ? ""
        : [
            includeVoice ? "voices" : null,
            includeControllers ? "CC" : null,
          ]
            .filter(Boolean)
            .join("+");
      useStudio.setState({
        view: next,
        selected: [],
        message: drumsOnly
          ? `Inserted drum pattern (CH9/10) · ${lib.name} · ${section.label} → ${destination.label}`
          : `Imported complete part${extras ? ` (+${extras})` : ""} · ${lib.name} · ${section.label} → ${destination.label}`,
      });
      close.current();
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  };

  const previewPhrase = async () => {
    if (!lib || !section) return;
    if (!output) {
      setError("Select a MIDI output to preview library phrases.");
      return;
    }
    setError(null);
    setPreviewing(true);
    try {
      await invoke("preview_library_phrase", {
        path: lib.path,
        librarySection: section.id,
        libraryChannel: sourceChannel,
        index: output.index,
        name: output.name,
      });
    } catch (e) {
      setError(String(e));
      setPreviewing(false);
    }
  };

  const stopPreview = async () => {
    try {
      await invoke("stop");
    } catch {
      /* ignore */
    }
    setPreviewing(false);
  };

  return (
    <div
      className="modal-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" && !pending && !busy) {
          e.preventDefault();
          close.current();
        }
      }}
    >
      <section
        ref={dialog}
        className="modal library-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="style-library-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={trapModalFocus}
      >
        <div className="modal-title">
          <h2 id="style-library-title">Style Part Library</h2>
          <button
            type="button"
            aria-label="Close"
            disabled={pending || busy}
            onClick={dismiss}
          >
            ×
          </button>
        </div>
        <p className="source-zone">
          Browse Style files or Patterns (section rows). Import notes into{" "}
          <strong>{destination.label}</strong>; optional Voice (MSB/LSB/PC)
          and MIDI controllers (CC). CASM/routing is never imported. Preview
          uses the selected MIDI output.
        </p>
        <div className="modal-actions" style={{ justifyContent: "flex-start" }}>
          <button
            type="button"
            disabled={pending || busy}
            onClick={() => void chooseFile()}
          >
            Open Style…
          </button>
          <button
            type="button"
            disabled={pending || busy}
            onClick={() => void chooseFolder()}
          >
            {pending && !lib ? "Scanning…" : "Choose folder…"}
          </button>
        </div>
        {catalog.length > 0 && (
          <>
            <div
              className="modal-actions"
              role="tablist"
              aria-label="Library tabs"
              style={{ justifyContent: "flex-start" }}
            >
              <button
                type="button"
                role="tab"
                aria-selected={tab === "files"}
                className={tab === "files" ? "primary" : "ghost"}
                disabled={pending || busy}
                onClick={() => setTab("files")}
              >
                Style files ({filteredStyles.length})
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={tab === "patterns"}
                className={tab === "patterns" ? "primary" : "ghost"}
                disabled={pending || busy}
                onClick={() => setTab("patterns")}
              >
                Patterns ({patterns.length})
              </button>
            </div>
            <label className="field">
              <span>Search</span>
              <input
                aria-label="Library search"
                type="search"
                value={search}
                disabled={pending || busy}
                placeholder={
                  tab === "patterns"
                    ? "Filter patterns…"
                    : "Filter Style files…"
                }
                onChange={(e) => setSearch(e.target.value)}
              />
            </label>
            <label className="mixer-scope">
              <input
                type="checkbox"
                checked={favoritesOnly}
                disabled={pending || busy}
                onChange={(e) => setFavoritesOnly(e.target.checked)}
              />
              Favorites only
            </label>
            {tab === "patterns" && (
              <label className="field">
                <span>Type</span>
                <select
                  aria-label="Pattern type"
                  value={patternType}
                  disabled={pending || busy}
                  onChange={(e) =>
                    setPatternType(e.target.value as PatternType)
                  }
                >
                  <option value="all">All patterns</option>
                  <option value="intro">Intro</option>
                  <option value="main">Main</option>
                  <option value="fill">Fill</option>
                  <option value="break">Break</option>
                  <option value="ending">Ending</option>
                </select>
              </label>
            )}
          </>
        )}
        {tab === "files" && filteredStyles.length > 0 && (
          <label className="field">
            <span>Style file ({filteredStyles.length})</span>
            <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
              <select
                aria-label="Library style file"
                value={lib?.path ?? ""}
                disabled={pending || busy}
                onChange={(e) => {
                  const next = filteredStyles.find(
                    (s) => s.path === e.target.value,
                  );
                  if (next) selectStyle(next);
                }}
              >
                {filteredStyles.map((s) => (
                  <option key={s.path} value={s.path}>
                    {favorites.includes(s.path) ? "★ " : ""}
                    {s.name} · {s.format} ·{" "}
                    {s.sections.reduce((n, x) => n + x.noteCount, 0)} notes
                  </option>
                ))}
              </select>
              {lib && (
                <button
                  type="button"
                  className="ghost"
                  disabled={pending || busy}
                  title={
                    favorites.includes(lib.path)
                      ? "Remove from favorites"
                      : "Add to favorites"
                  }
                  aria-label={
                    favorites.includes(lib.path)
                      ? "Remove from favorites"
                      : "Add to favorites"
                  }
                  onClick={() => toggleFavorite(lib.path)}
                >
                  {favorites.includes(lib.path) ? "★" : "☆"}
                </button>
              )}
            </div>
          </label>
        )}
        {tab === "patterns" && patterns.length > 0 && (
          <label className="field">
            <span>Pattern ({patterns.length})</span>
            <select
              aria-label="Library pattern"
              value={
                lib && section
                  ? `${lib.path}::${section.id}`
                  : patterns[0]?.key ?? ""
              }
              disabled={pending || busy}
              onChange={(e) => {
                const row = patterns.find((p) => p.key === e.target.value);
                if (row) selectPattern(row);
              }}
            >
              {patterns.map((p) => (
                <option key={p.key} value={p.key}>
                  {favorites.includes(p.style.path) ? "★ " : ""}
                  {p.style.name} · {p.section.label} ·{" "}
                  {p.section.bars.toFixed(1)} bars · {p.section.noteCount} notes
                </option>
              ))}
            </select>
          </label>
        )}
        {tab === "patterns" && catalog.length > 0 && patterns.length === 0 && (
          <p className="source-zone">No matching Style parts.</p>
        )}
        {lib && (
          <fieldset className="field" disabled={pending || busy}>
            <p className="source-zone">
              {lib.name} · {lib.format} · {lib.ppq} PPQ
            </p>
            <label>
              <span>Library section</span>
              <select
                aria-label="Library section"
                value={sectionId}
                onChange={(e) => {
                  const id = Number(e.target.value);
                  setSectionId(id);
                  const s = lib.sections.find((x) => x.id === id);
                  if (s?.channels.length) setSourceChannel(s.channels[0]);
                }}
              >
                {lib.sections.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.label} · {s.bars.toFixed(1)} bars · {s.noteCount} notes
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Source channel</span>
              <select
                aria-label="Library source channel"
                value={sourceChannel}
                onChange={(e) => setSourceChannel(Number(e.target.value))}
              >
                {(section?.channels.length
                  ? section.channels
                  : Array.from({ length: 16 }, (_, i) => i + 1)
                ).map((ch) => (
                  <option key={ch} value={ch}>
                    CH{ch}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Timing</span>
              <select
                aria-label="Library timing mode"
                value={fit}
                onChange={(e) =>
                  setFit(e.target.value as "keep" | "fit" | "repeat")
                }
              >
                <option value="keep">Keep (reject if longer)</option>
                <option value="fit">Fit (stretch/compress)</option>
                <option value="repeat">Repeat (tile)</option>
              </select>
            </label>
            <label className="mixer-scope">
              <input
                type="checkbox"
                checked={includeVoice}
                disabled={pending || busy}
                onChange={(e) => setIncludeVoice(e.target.checked)}
              />
              Import voice (MSB/LSB/PC)
            </label>
            <label className="mixer-scope">
              <input
                type="checkbox"
                checked={includeControllers}
                disabled={pending || busy}
                onChange={(e) => setIncludeControllers(e.target.checked)}
              />
              Import controllers (CC) — CASM unchanged
            </label>
          </fieldset>
        )}
        {error && (
          <p role="alert" className="source-error">
            {error}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" disabled={pending || busy} onClick={dismiss}>
            Cancel
          </button>
          {previewing ? (
            <button
              type="button"
              disabled={busy}
              onClick={() => void stopPreview()}
            >
              Stop preview
            </button>
          ) : (
            <button
              type="button"
              disabled={pending || busy || !lib || !section}
              title="Audition library source notes on the selected MIDI output"
              onClick={() => void previewPhrase()}
            >
              Play preview
            </button>
          )}
          <button
            type="button"
            disabled={pending || busy || !lib || !section}
            title="Replace only Rhy1/Rhy2 (CH9/10) notes; other parts and CASM unchanged"
            onClick={() => void importCompletePart(true)}
          >
            Insert drum pattern
          </button>
          <button
            type="button"
            disabled={pending || busy || !lib || !section}
            title="Replace notes on every source channel; optional voices; CASM unchanged"
            onClick={() => void importCompletePart(false)}
          >
            Import complete part
          </button>
          <button
            type="button"
            className="primary"
            disabled={pending || busy || !lib || !section}
            onClick={() => void importPhrase()}
          >
            {pending ? "Importing…" : "Import notes"}
          </button>
        </div>
      </section>
    </div>
  );
}
