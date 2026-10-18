import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { PackageOpen, FolderOpen, FileSearch, Download, X } from "lucide-react";
import { useStudio } from "./store";
import type { CatalogVoice } from "./contracts";

type Capability = { category: string; status: string; detail: string };
type PackItem = {
  title?: string;
  extension?: string;
  fbinKind: string;
  fbinSize: number;
  status: string;
};
type PackEntry = { title?: string; items: PackItem[] };
type SectionInfo = {
  id: string;
  offset: number;
  size: number;
  status: string;
  note: string;
};
type RecoveryReport = {
  installation?: {
    path: string;
    fileSize: number;
    header: { model?: string; charset?: string; packIdHex?: string };
    sections: SectionInfo[];
    packs: PackEntry[];
    diagnostics: string[];
  };
  instrument?: {
    modelName: string;
    instrumentId: string;
    compositeId: string;
    packInstallDataIdentification: number;
    packInstallDataFormatType: number;
  };
  project?: {
    title?: string;
    fileSize: number;
    resources: {
      title?: string;
      extension?: string;
      kind: string;
      size: number;
    }[];
  };
  vprs?: {
    banner?: string;
    note: string;
    header?: { field0: number; field1: number; field2: number; dataBase: number };
  };
  waveIndex?: { path: string; entries: { waveKey: string; startAddress: number; waveSize: number }[]; note: string };
  voiceMaps?: {
    packTitle?: string;
    itemTitle?: string;
    addresses: { channel: number; msb: number; lsb: number; program: number }[];
  }[];
  capabilities: Capability[];
  blockers: string[];
  notes: string[];
};

export function ExpansionRecovery({ onClose }: { onClose: () => void }) {
  const setExpansionVoices = useStudio((s) => s.setExpansionVoices);
  const [ppiPath, setPpiPath] = useState<string | null>(null);
  const [n27Path, setN27Path] = useState<string | null>(null);
  const [ppfPath, setPpfPath] = useState<string | null>(null);
  const [waveIndexPath, setWaveIndexPath] = useState<string | null>(null);
  const [encryptionKeyHex, setEncryptionKeyHex] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<RecoveryReport | null>(null);
  const [extractMsg, setExtractMsg] = useState<string | null>(null);

  const pick = async (filters: { name: string; extensions: string[] }[]) => {
    if (!isTauri()) return null;
    const selected = await open({ multiple: false, filters });
    return typeof selected === "string" ? selected : null;
  };

  const analyze = async () => {
    setBusy(true);
    setError(null);
    setExtractMsg(null);
    try {
      if (!isTauri()) throw new Error("Expansion recovery requires the desktop app");
      const next = await invoke<RecoveryReport>("analyze_expansion_pack", {
        ppiPath,
        n27Path,
        ppfPath,
        waveIndexPath,
        encryptionKeyHex: encryptionKeyHex.trim() || null,
      });
      setReport(next);
      const voices: CatalogVoice[] = [];
      const seen = new Set<string>();
      for (const map of next.voiceMaps ?? []) {
        for (const a of map.addresses) {
          const key = `${a.msb}-${a.lsb}-${a.program}`;
          if (seen.has(key)) continue;
          seen.add(key);
          voices.push({
            category: "Expansion",
            subcategory: map.packTitle ?? map.itemTitle ?? "-",
            name:
              map.itemTitle ??
              `Expansion ${a.msb}/${a.lsb}/${a.program + 1}`,
            msb: a.msb,
            lsb: a.lsb,
            program: a.program,
            voiceType: "ExpansionPack",
            page: 0,
          });
        }
      }
      setExpansionVoices(voices);
    } catch (e) {
      setReport(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const extractPpi = async (index: number) => {
    if (!ppiPath || !isTauri()) return;
    setBusy(true);
    setExtractMsg(null);
    try {
      const dest = await open({ directory: true, multiple: false });
      if (typeof dest !== "string") return;
      const out = await invoke<string>("extract_expansion_resource", {
        request: {
          sourcePath: ppiPath,
          destDir: dest,
          kind: "ppi",
          resourceIndex: index,
        },
      });
      setExtractMsg(`Extracted ${out}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const extractPpf = async (index: number) => {
    if (!ppfPath || !isTauri()) return;
    setBusy(true);
    setExtractMsg(null);
    try {
      const dest = await open({ directory: true, multiple: false });
      if (typeof dest !== "string") return;
      const out = await invoke<string>("extract_expansion_resource", {
        request: {
          sourcePath: ppfPath,
          destDir: dest,
          kind: "ppf",
          resourceIndex: index,
        },
      });
      setExtractMsg(`Extracted ${out}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const rewritePpf = async () => {
    if (!ppfPath || !isTauri()) return;
    setBusy(true);
    setExtractMsg(null);
    try {
      const base =
        ppfPath.replace(/^.*[\\/]/, "").replace(/\.ppf$/i, "") || "pack";
      const destPath = await save({
        defaultPath: `${base}.rewrite.ppf`,
        filters: [{ name: "Pack Project", extensions: ["ppf"] }],
      });
      if (typeof destPath !== "string") return;
      const out = await invoke<string>("rewrite_expansion_ppf", {
        sourcePath: ppfPath,
        destPath,
      });
      setExtractMsg(`Rewrote structural .ppf to ${out}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" role="presentation" onClick={onClose}>
      <div
        className="modal expansion-recovery"
        role="dialog"
        aria-modal="true"
        aria-labelledby="expansion-recovery-title"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            onClose();
          }
        }}
      >
        <header className="modal-header">
          <h2 id="expansion-recovery-title">
            <PackageOpen size={18} /> Expansion Pack Recovery
          </h2>
          <button className="icon-button" aria-label="Close" onClick={onClose}>
            <X size={16} />
          </button>
        </header>

        <p className="modal-lead">
          Analyze Yamaha installation (`.ppi`/`.pqi`) and project (`.ppf`)
          files. VWST samples use the recovered SME stream cipher; the 16-byte
          seed comes from offline Model Encryption (encrypt context /{" "}
          <code>CESC</code>), not a live SSL registration fetch and not the{" "}
          <code>.n27</code> alone.
        </p>

        <ol className="recovery-steps">
          <li>
            <strong>1. Installation package</strong>
            <div className="recovery-row">
              <code>{ppiPath ?? "No .ppi/.pqi selected"}</code>
              <button
                disabled={busy}
                onClick={() =>
                  void pick([
                    {
                      name: "Installation",
                      extensions: ["ppi", "pqi"],
                    },
                  ]).then((p) => p && setPpiPath(p))
                }
              >
                <FolderOpen size={14} /> Select
              </button>
            </div>
          </li>
          <li>
            <strong>2. Instrument information (optional)</strong>
            <div className="recovery-row">
              <code>{n27Path ?? "Optional .n27 — not required to read packages"}</code>
              <button
                disabled={busy}
                onClick={() =>
                  void pick([
                    { name: "Instrument Info", extensions: ["n27"] },
                  ]).then((p) => p && setN27Path(p))
                }
              >
                <FolderOpen size={14} /> Select
              </button>
            </div>
          </li>
          <li>
            <strong>3. Pack project (optional)</strong>
            <div className="recovery-row">
              <code>
                {ppfPath ??
                  "Optional source .ppf (AIFF/UVF extract when present)"}
              </code>
              <button
                disabled={busy}
                onClick={() =>
                  void pick([{ name: "Pack Project", extensions: ["ppf"] }]).then(
                    (p) => p && setPpfPath(p),
                  )
                }
              >
                <FolderOpen size={14} /> Select
              </button>
            </div>
          </li>
          <li>
            <strong>4. YEM wave index (optional)</strong>
            <div className="recovery-row">
              <code>
                {waveIndexPath ??
                  "Optional InstallTargets/.../.waves catalog for VWST offsets"}
              </code>
              <button
                disabled={busy}
                onClick={() =>
                  void pick([
                    { name: "Wave index", extensions: ["waves", "xml"] },
                  ]).then((p) => p && setWaveIndexPath(p))
                }
              >
                <FolderOpen size={14} /> Select
              </button>
            </div>
          </li>
          <li>
            <strong>5. Offline SME seed (optional)</strong>
            <div className="recovery-row">
              <input
                className="recovery-key-input"
                spellCheck={false}
                placeholder="32 hex chars — offline SME seed (Model Encryption)"
                value={encryptionKeyHex}
                onChange={(e) => setEncryptionKeyHex(e.target.value)}
                disabled={busy}
              />
            </div>
          </li>
        </ol>

        <div className="recovery-actions">
          <button
            className="primary"
            disabled={busy || (!ppiPath && !ppfPath)}
            onClick={() => void analyze()}
          >
            <FileSearch size={14} /> Analyze package
          </button>
        </div>

        {error && <p className="error-banner">{error}</p>}
        {extractMsg && <p className="ok-banner">{extractMsg}</p>}

        {report && (
          <div className="recovery-report">
            {report.instrument && (
              <section>
                <h3>Instrument</h3>
                <p>
                  {report.instrument.modelName} · {report.instrument.compositeId}{" "}
                  · format {report.instrument.packInstallDataFormatType}
                </p>
              </section>
            )}

            <section>
              <h3>Capabilities</h3>
              <ul className="capability-list">
                {report.capabilities.map((c) => (
                  <li key={c.category}>
                    <span className={`cap-status ${c.status}`}>{c.status}</span>
                    <strong>{c.category}</strong>
                    <small>{c.detail}</small>
                  </li>
                ))}
              </ul>
            </section>

            {report.blockers.length > 0 && (
              <section>
                <h3>Blockers</h3>
                <ul>
                  {report.blockers.map((b) => (
                    <li key={b}>{b}</li>
                  ))}
                </ul>
              </section>
            )}

            {report.vprs?.banner && (
              <section>
                <h3>VPRS</h3>
                <p>
                  {report.vprs.banner}
                  {report.vprs.header
                    ? ` · data_base=0x${report.vprs.header.dataBase.toString(16)}`
                    : ""}{" "}
                  — {report.vprs.note}
                </p>
              </section>
            )}

            {report.waveIndex && (
              <section>
                <h3>YEM wave index</h3>
                <p>
                  {report.waveIndex.entries.length} entries ·{" "}
                  {report.waveIndex.path}
                </p>
              </section>
            )}

            {report.voiceMaps && report.voiceMaps.length > 0 && (
              <section>
                <h3>Expansion voice map</h3>
                <p>
                  {report.voiceMaps.reduce((n, m) => n + m.addresses.length, 0)}{" "}
                  bank/program addresses from PACK MIDI (for voice picker).
                </p>
                <ul className="pack-list">
                  {report.voiceMaps.slice(0, 24).map((m, idx) => (
                    <li key={`${m.itemTitle}-${idx}`}>
                      <span>
                        {m.packTitle ?? "pack"} / {m.itemTitle ?? "item"}:{" "}
                        {m.addresses
                          .map(
                            (a) =>
                              `ch${a.channel} ${a.msb}/${a.lsb}/${a.program + 1}`,
                          )
                          .join(", ")}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            )}

            {report.notes.map((n) => (
              <p key={n} className="recovery-note">
                {n}
              </p>
            ))}

            {report.installation && (
              <section>
                <h3>
                  Installation · {report.installation.header.model ?? "?"} ·{" "}
                  {(report.installation.fileSize / (1024 * 1024)).toFixed(1)} MiB
                </h3>
                <div className="section-chips">
                  {report.installation.sections.map((s) => (
                    <span key={`${s.id}-${s.offset}`} title={s.note}>
                      {s.id} ({s.status})
                    </span>
                  ))}
                </div>
                <div className="pack-list">
                  {report.installation.packs.map((pack, pIdx) => {
                    const base = report.installation!.packs
                      .slice(0, pIdx)
                      .reduce((n, p) => n + p.items.length, 0);
                    return (
                      <details key={`${pack.title}-${pIdx}`}>
                        <summary>
                          {pack.title ?? "(untitled pack)"} · {pack.items.length}{" "}
                          items
                        </summary>
                        <ul>
                          {pack.items.map((item, iIdx) => {
                            const resourceIndex = base + iIdx;
                            return (
                              <li key={`${item.title}-${resourceIndex}`}>
                                <span>
                                  {item.title ?? "item"} ·{" "}
                                  {item.extension ?? "?"} · {item.fbinKind} ·{" "}
                                  {item.fbinSize} B
                                </span>
                                {item.fbinKind === "midi" && (
                                  <button
                                    disabled={busy}
                                    onClick={() => void extractPpi(resourceIndex)}
                                  >
                                    <Download size={13} /> Extract
                                  </button>
                                )}
                              </li>
                            );
                          })}
                        </ul>
                      </details>
                    );
                  })}
                </div>
              </section>
            )}

            {report.project && (
              <section>
                <h3>
                  Project · {report.project.title ?? "?"} ·{" "}
                  {report.project.resources.length} resources
                </h3>
                <div className="recovery-row" style={{ marginBottom: 8 }}>
                  <button disabled={busy} onClick={() => void rewritePpf()}>
                    <Download size={13} /> Rewrite structural .ppf
                  </button>
                </div>
                <ul className="pack-list">
                  {report.project.resources.map((r, idx) => (
                    <li key={`${r.title}-${idx}`}>
                      <span>
                        {r.title ?? "resource"} · {r.kind} · {r.size} B
                      </span>
                      {(r.kind === "aiff" || r.kind === "uvfXml") && (
                        <button
                          disabled={busy}
                          onClick={() => void extractPpf(idx)}
                        >
                          <Download size={13} /> Extract
                        </button>
                      )}
                    </li>
                  ))}
                </ul>
              </section>
            )}

            <section>
              <h3>PPF reconstruction</h3>
              <p>
                {report.capabilities.some((c) => c.status === "keyed-decrypt-ready")
                  ? "SME decrypt is ready: supply VWST offsets (from a YEM .waves index) via extract kind=vwst."
                  : report.blockers.some((b) => b.includes("encryptionkey") || b.includes("SME seed") || b.includes("samples") || b.includes("offline"))
                    ? "Blocked for installation-only recovery: VWST still needs the offline SME seed (or a source .ppf)."
                    : report.project
                      ? "Structural .ppf rewrite is available (XPFH+BLOB round-trip). YEM import of rewritten files is not yet verified."
                      : "No .ppf loaded."}
              </p>
            </section>
          </div>
        )}
      </div>
    </div>
  );
}
