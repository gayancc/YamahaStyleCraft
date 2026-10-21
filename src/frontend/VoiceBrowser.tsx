import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Pencil, Trash2 } from "lucide-react";
import { useStudio } from "./store";
import { FloatingToolWindow } from "./FloatingToolWindow";
import { UserVoiceEditor } from "./UserVoiceEditor";
import { isUserVoice } from "./userVoices";
import { importInstalledPacks, isKit } from "./installedPacks";
import type {
  CatalogSummary,
  CatalogVoice,
  Section,
  View,
  VoiceSearch,
} from "./contracts";

export function VoiceName({
  section,
  channel,
  onOpen,
}: {
  section: Section;
  channel: number;
  onOpen: () => void;
}) {
  const { view } = useStudio();
  const [voice, setVoice] = useState<CatalogVoice | null>(null);
  useEffect(() => {
    let live = true;
    setVoice(null);
    if (view)
      void invoke<CatalogVoice | null>("resolve_catalog_voice", {
        section: section.id,
        channel,
        revision: view.revision,
      })
        .then((v) => {
          if (live) setVoice(v);
        })
        .catch(() => {});
    return () => {
      live = false;
    };
  }, [view, section.id, channel]);
  return (
    <button
      className="voice-picker"
      onClick={onOpen}
      aria-label="Browse Genos voices"
    >
      {voice?.name ?? "Browse voices…"}
    </button>
  );
}

export function VoiceBrowser({
  section,
  channel,
  midiInput,
  monitoringActive = false,
  onClose,
}: {
  section: Section;
  channel: number;
  midiInput?: string;
  monitoringActive?: boolean;
  onClose: () => void;
}) {
  const { view, expansionVoices, userVoices, setUserVoices } = useStudio();
  const [editing, setEditing] = useState<CatalogVoice | "new" | null>(null);
  const [original] = useState(view!);
  const [catalog, setCatalog] = useState<CatalogSummary | null>(null);
  const [current, setCurrent] = useState<CatalogVoice | null>(null);
  const [selected, setSelected] = useState<CatalogVoice | null>(null);
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [category, setCategory] = useState("");
  const [offset, setOffset] = useState(0);
  const pageSize = 40;
  const [result, setResult] = useState<VoiceSearch | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [searching, setSearching] = useState(false);
  const working = useRef(false),
    dialog = useRef<HTMLDivElement>(null),
    close = useRef(onClose);
  const catalogVersion = useRef(0);
  close.current = onClose;
  const stale = view !== original;
  const args = { section: section.id, channel, revision: original.revision };
  const extraVoices = [...userVoices, ...expansionVoices];
  const extraKinds = ["all", "expansion", "voices", "drums"];
  const expansionMatches = extraVoices.filter(
    (v) =>
      (kind !== "drums" || isKit(v)) &&
      (kind !== "voices" || !isKit(v)) &&
      (!category || category === v.category) &&
      query
        .toLowerCase()
        .split(/\s+/)
        .every((term) =>
          `${v.name} ${v.subcategory} ${v.category} ${v.msb} ${v.lsb} ${v.program + 1}`
            .toLowerCase()
            .includes(term),
        ),
  );
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.focus();
    let live = true;
    const version = catalogVersion.current;
    void invoke<CatalogSummary | null>("voice_catalog_status")
      .then((value) => {
        if (live && catalogVersion.current === version) setCatalog(value);
      })
      .catch((e) => {
        if (live && catalogVersion.current === version) setError(String(e));
      });
    void invoke<CatalogVoice | null>("resolve_catalog_voice", args)
      .then((value) => {
        if (live && catalogVersion.current === version) setCurrent(value);
      })
      .catch(() => {});
    return () => {
      live = false;
      previous?.focus();
    };
    // Target context is deliberately fixed for the lifetime of this dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (pending) dialog.current?.focus();
  }, [pending]);
  useEffect(() => {
    let live = true;
    setSelected(null);
    setResult(null);
    if (!catalog) {
      setSearching(false);
      return;
    }
    setSearching(true);
    const timer = setTimeout(() => {
      if (kind === "expansion") {
        if (live) {
          setResult({
            total: expansionMatches.length,
            voices: expansionMatches.slice(offset, offset + pageSize),
            offset,
            hasMore: offset + pageSize < expansionMatches.length,
          });
          setSearching(false);
        }
        return;
      }
      void invoke<VoiceSearch>("search_voice_catalog", {
        query,
        kind,
        category,
        offset,
        limit: pageSize,
      })
        .then((value) => {
          if (live) {
            setResult(value);
            setSearching(false);
          }
        })
        .catch((e) => {
          if (live) {
            setError(String(e));
            setSearching(false);
          }
        });
    }, 180);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [query, catalog, kind, category, offset, expansionVoices, userVoices]);
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
      throw new Error(
        "The document changed. Close and reopen the voice browser.",
      );
  };
  const importPdf = () =>
    run(async () => {
      const path = await open({
        multiple: false,
        filters: [{ name: "Genos Data List PDF", extensions: ["pdf"] }],
      });
      if (typeof path !== "string") return;
      const imported = await invoke<CatalogSummary>("import_voice_catalog", {
        path,
      });
      catalogVersion.current += 1;
      setCatalog(imported);
      setSelected(null);
      setQuery("");
      setOffset(0);
      const value = await invoke<CatalogVoice | null>(
        "resolve_catalog_voice",
        args,
      ).catch(() => null);
      setCurrent(value);
      useStudio.setState({ message: "Genos voice catalog imported locally" });
    });
  const apply = () =>
    run(async () => {
      ensureCurrent();
      if (!selected) return;
      const address = {
        msb: selected.msb,
        lsb: selected.lsb,
        program: selected.program,
      };
      const next = extraVoices.includes(selected)
        ? await invoke<View>("edit_document", {
            revision: original.revision,
            docLabel: "main",
            command: {
              type: "setVoice",
              section: section.id,
              channel,
              address,
            },
          })
        : await invoke<View>("apply_catalog_voice", { ...args, address });
      useStudio.setState({
        view: next,
        message:
          next.revision === original.revision
            ? "Voice unchanged"
            : `${selected.name} applied · one undo step`,
      });
      close.current();
    });
  const dismiss = () => {
    if (!working.current) close.current();
  };
  return (
    <FloatingToolWindow
      open
      title="Genos voices"
      context={`${section.label} · Source ${channel} · Section-start voice`}
      className="voice-picker-window"
      onClose={dismiss}
    >
      <div tabIndex={-1} ref={dialog}>
        <div className="voice-browser-body">
          <p className="muted" role="status" aria-label="Voice catalog">
            {catalog
              ? `${catalog.count.toLocaleString()} factory voices · Genos v2.00`
              : "Loading Genos voices…"}
          </p>
          <details className="voice-catalog-tools">
            <summary>Catalog tools</summary>
            <div className="catalog-import">
              <button onClick={() => void importPdf()} disabled={pending}>
                Import Data List PDF…
              </button>
              <p>
                Optional: replace the local catalog from the supported Genos
                v2.00 Data List. Factory voices are included with the
                application.
              </p>
            </div>
          </details>
          {extraKinds.includes(kind) && (
            <div
              className={`voice-results my-voices${kind === "expansion" ? " tall" : ""}`}
              aria-label="My and expansion voices"
            >
              <div className="my-voices-head">
                <p className="muted">
                  {expansionMatches.length
                    ? `${expansionMatches.length} custom / expansion voice${expansionMatches.length === 1 ? "" : "s"}`
                    : "Import your installed expansion packs to list their voices and kits here."}
                </p>
                <button
                  type="button"
                  disabled={pending || stale}
                  title="Read every voice and kit from your Expansion Manager installation file (e.g. Genos_PackInstallData.pqi)"
                  onClick={() =>
                    void run(async () => {
                      const packs = await importInstalledPacks();
                      if (packs)
                        useStudio.setState({
                          message: `${packs.voices.length} installed voices and kits imported`,
                        });
                    })
                  }
                >
                  Import installed packs…
                </button>
                <button
                  type="button"
                  disabled={pending || stale || editing !== null}
                  onClick={() => setEditing("new")}
                >
                  + Add voice
                </button>
              </div>
              {editing !== null && (
                <UserVoiceEditor
                  initial={editing === "new" ? null : editing}
                  midiInput={midiInput}
                  monitoringActive={monitoringActive}
                  onCancel={() => setEditing(null)}
                  onSave={(voice) => {
                    const without =
                      editing === "new"
                        ? userVoices
                        : userVoices.filter((v) => v !== editing);
                    const clash = without.some(
                      (v) =>
                        v.msb === voice.msb &&
                        v.lsb === voice.lsb &&
                        v.program === voice.program,
                    );
                    if (clash) {
                      setError("A saved voice already uses that bank/program.");
                      return;
                    }
                    setError(null);
                    setUserVoices([...without, voice]);
                    setSelected(null);
                    setEditing(null);
                  }}
                />
              )}
              {expansionMatches.map((v) => (
                <div
                  className="voice-row"
                  key={`exp-${v.voiceType}-${v.msb}-${v.lsb}-${v.program}`}
                >
                  <button
                    aria-pressed={selected === v}
                    disabled={pending || stale}
                    onClick={() => {
                      setSelected(v);
                      setError(null);
                    }}
                  >
                    <span>
                      <strong>{v.name}</strong>
                      <small>
                        {v.category}
                        {v.subcategory !== "-"
                          ? ` / ${v.subcategory}`
                          : ""} · {isKit(v) ? "Kit" : "Voice"}
                      </small>
                    </span>
                    <span className="voice-address">
                      {v.msb} / {v.lsb} / {v.program + 1}
                    </span>
                  </button>
                  {isUserVoice(v) && (
                    <span className="voice-row-tools">
                      <button
                        type="button"
                        aria-label={`Edit ${v.name}`}
                        title="Edit"
                        disabled={pending || editing !== null}
                        onClick={() => setEditing(v)}
                      >
                        <Pencil size={13} />
                      </button>
                      <button
                        type="button"
                        className="danger-link"
                        aria-label={`Remove ${v.name}`}
                        title="Remove from my voices"
                        disabled={pending}
                        onClick={() => {
                          if (selected === v) setSelected(null);
                          setUserVoices(userVoices.filter((u) => u !== v));
                        }}
                      >
                        <Trash2 size={13} />
                      </button>
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
          {catalog && (
            <>
              <p className="muted">
                Current:{" "}
                {current
                  ? `${current.name} · MSB ${current.msb} / LSB ${current.lsb} / Program ${current.program + 1}`
                  : "No catalog match for the explicit voice at this section start"}
              </p>
              <label className="voice-search">
                <span>Search voices</span>
                <input
                  autoFocus
                  aria-label="Search voices"
                  placeholder="Name, category or type…"
                  value={query}
                  maxLength={120}
                  disabled={pending}
                  onChange={(e) => {
                    setError(null);
                    setQuery(e.target.value);
                    setOffset(0);
                  }}
                />
              </label>
              <div className="voice-browser-filters">
                <label>
                  Bank group
                  <select
                    aria-label="Voice bank group"
                    value={kind}
                    disabled={pending}
                    onChange={(e) => {
                      setKind(e.target.value);
                      setCategory("");
                      setOffset(0);
                    }}
                  >
                    <option value="all">All Genos voices</option>
                    <option value="voices">Voices (no kits)</option>
                    <option value="panel">Panel voices</option>
                    <option value="xg">XG / GM</option>
                    <option value="gm2">GM2</option>
                    <option value="drums">Drum / SFX kits</option>
                    <option value="expansion">My voices / Expansion</option>
                  </select>
                </label>
                <label>
                  Category
                  <select
                    aria-label="Voice category"
                    value={category}
                    disabled={pending}
                    onChange={(e) => {
                      setCategory(e.target.value);
                      setOffset(0);
                    }}
                  >
                    <option value="">All categories</option>
                    {catalog.categories?.map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                    {userVoices.length > 0 && (
                      <option value="My voices">My voices</option>
                    )}
                    {expansionVoices.length > 0 && (
                      <option value="Expansion">Expansion</option>
                    )}
                  </select>
                </label>
              </div>
              <p className="muted" role="status" hidden={kind === "expansion"}>
                {searching
                  ? "Searching…"
                  : result
                    ? result.total
                      ? `${offset + 1}–${Math.min(offset + pageSize, result.total)} of ${result.total} voices`
                      : "0 matches"
                    : ""}
              </p>
              <div
                className="voice-results"
                hidden={kind === "expansion"}
                aria-label="Voice results"
                aria-busy={searching}
              >
                {result?.voices.map((v) => (
                  <button
                    key={`${v.msb}-${v.lsb}-${v.program}`}
                    aria-pressed={selected === v}
                    disabled={pending || stale}
                    onClick={() => {
                      setSelected(v);
                      setError(null);
                    }}
                  >
                    <span>
                      <strong>{v.name}</strong>
                      <small>
                        {v.category}
                        {v.subcategory !== "-"
                          ? ` / ${v.subcategory}`
                          : ""} · {v.voiceType}
                      </small>
                    </span>
                    <span className="voice-address">
                      {v.msb} / {v.lsb} / {v.program + 1}
                    </span>
                  </button>
                ))}
                {!searching && result?.total === 0 && (
                  <p className="muted">
                    No voices match. Try fewer search terms.
                  </p>
                )}
              </div>
              <nav
                className="voice-page-controls"
                hidden={kind === "expansion"}
                aria-label="Voice result pages"
              >
                <button
                  disabled={pending || searching || offset === 0}
                  onClick={() => setOffset(Math.max(0, offset - pageSize))}
                >
                  Previous voices
                </button>
                <button
                  disabled={
                    pending ||
                    searching ||
                    !result ||
                    offset + pageSize >= result.total
                  }
                  onClick={() => setOffset(offset + pageSize)}
                >
                  Next voices
                </button>
              </nav>
            </>
          )}
          {selected && (
            <div className="voice-confirm" aria-label="Voice selection preview">
              <strong>{selected.name}</strong>
              <p>
                MSB {selected.msb} · LSB {selected.lsb} · Program{" "}
                {selected.program + 1}
                {selected.page > 0
                  ? ` (Data List page ${selected.page})`
                  : selected.category === "Expansion"
                    ? " (expansion pack recovery)"
                    : ""}
              </p>
              <p>
                Applies bank/program events at {section.label} start, source{" "}
                {channel}. Later events stay in the file; following sections
                inherit these values until changed. No MIDI is sent.
              </p>
              {selected.category === "DrumKit" && (
                <p>A kit Program Change can reset per-key Drum Setup.</p>
              )}
            </div>
          )}
          {(error || stale) && (
            <p className="source-error" role="alert">
              {stale
                ? "The document changed. Close and reopen the voice browser."
                : error}
            </p>
          )}
        </div>
        <footer className="modal-actions">
          <button onClick={dismiss} disabled={pending}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={pending || stale || !selected || searching}
            onClick={() => void apply()}
          >
            {pending ? "Working…" : "Apply voice"}
          </button>
        </footer>
      </div>
    </FloatingToolWindow>
  );
}
