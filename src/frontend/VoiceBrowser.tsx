import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useStudio } from "./store";
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
  onClose,
}: {
  section: Section;
  channel: number;
  onClose: () => void;
}) {
  const { view } = useStudio();
  const [original] = useState(view!);
  const [catalog, setCatalog] = useState<CatalogSummary | null>(null);
  const [current, setCurrent] = useState<CatalogVoice | null>(null);
  const [selected, setSelected] = useState<CatalogVoice | null>(null);
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<VoiceSearch | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [searching, setSearching] = useState(false);
  const working = useRef(false),
    dialog = useRef<HTMLElement>(null),
    close = useRef(onClose);
  const catalogVersion = useRef(0);
  close.current = onClose;
  const stale = view !== original;
  const args = { section: section.id, channel, revision: original.revision };
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
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
      void invoke<VoiceSearch>("search_voice_catalog", { query })
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
  }, [query, catalog]);
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
      const next = await invoke<View>("apply_catalog_voice", {
        ...args,
        address,
      });
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
    <div
      className="modal-backdrop"
      onPointerDown={(e) => {
        if (e.currentTarget === e.target) dismiss();
      }}
    >
      <section
        className="modal voice-browser"
        role="dialog"
        aria-modal="true"
        aria-labelledby="voice-browser-title"
        tabIndex={-1}
        ref={dialog}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") dismiss();
          if (e.key === "Tab") {
            const controls = [
                ...dialog.current!.querySelectorAll<HTMLElement>(
                  'button:not(:disabled),input:not(:disabled),summary,[tabindex="0"]',
                ),
              ],
              first = controls[0],
              last = controls.at(-1);
            if (!first) {
              e.preventDefault();
              return;
            }
            if (document.activeElement === dialog.current) {
              e.preventDefault();
              (e.shiftKey ? last : first)?.focus();
              return;
            }
            if (e.shiftKey && document.activeElement === first) {
              e.preventDefault();
              last?.focus();
            }
            if (!e.shiftKey && document.activeElement === last) {
              e.preventDefault();
              first.focus();
            }
          }
        }}
      >
        <header>
          <h2 id="voice-browser-title">Genos voices</h2>
          <p>
            {section.label} · Source {channel} · Section-start voice
          </p>
        </header>
        <div className="voice-browser-body">
          <div className="catalog-import">
            <button onClick={() => void importPdf()} disabled={pending}>
              Import Data List PDF…
            </button>
            <p>
              {catalog
                ? `${catalog.count.toLocaleString()} voices · ${catalog.edition}`
                : "Import Yamaha’s Genos v2.00 English Data List (11/2019), genos_en_dl_f0.pdf. Up to 32 MiB; other editions are not supported yet."}
            </p>
          </div>
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
                  aria-label="Search voices"
                  placeholder="Name, category or type…"
                  value={query}
                  maxLength={120}
                  disabled={pending}
                  onChange={(e) => {
                    setError(null);
                    setQuery(e.target.value);
                  }}
                />
              </label>
              <p className="muted" role="status">
                {searching
                  ? "Searching…"
                  : result
                    ? `${result.total} ${result.total === 1 ? "match" : "matches"}${result.total > 100 ? " · showing first 100; refine your search" : ""}`
                    : ""}
              </p>
              <div
                className="voice-results"
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
            </>
          )}
          {selected && (
            <div className="voice-confirm" aria-label="Voice selection preview">
              <strong>{selected.name}</strong>
              <p>
                MSB {selected.msb} · LSB {selected.lsb} · Program{" "}
                {selected.program + 1} (Data List page {selected.page})
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
      </section>
    </div>
  );
}
