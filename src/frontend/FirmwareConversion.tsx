import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface FirmwareStatus {
  active: boolean;
  path: string | null;
}

const INACTIVE: FirmwareStatus = { active: false, path: null };

function fileName(path: string | null): string {
  return path?.split(/[\\/]/).pop() ?? "";
}

/**
 * Chooses the user's own Genos 2.13 update file, which turns on exact firmware chord
 * conversion. The app never bundles firmware; it reads only the file chosen here.
 */
export function FirmwareConversion() {
  const [status, setStatus] = useState<FirmwareStatus>(INACTIVE);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    invoke<FirmwareStatus>("firmware_status")
      .then((next) => {
        if (live && next) setStatus(next);
      })
      .catch((e: unknown) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, []);

  const choose = async () => {
    const path = await open({
      multiple: false,
      title: "Select your Genos 2.13 update file",
      filters: [{ name: "Genos update file", extensions: ["prg", "elf"] }],
    });
    if (typeof path !== "string") return;
    setBusy(true);
    setProblem(null);
    try {
      setStatus(await invoke<FirmwareStatus>("configure_firmware", { path }));
    } catch (e: unknown) {
      setProblem(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="firmware-conversion">
      <span
        className={`connection-dot ${status.active ? "connected" : ""}`}
        aria-hidden="true"
      />
      <span role="status">
        {status.active
          ? `Exact Genos 2.13 conversion · ${fileName(status.path)}`
          : "Approximate chord conversion · no Genos update file loaded"}
      </span>
      <button
        type="button"
        disabled={busy}
        onClick={() => void choose()}
        title="Use your own Genos 2.13 update file for exact chord conversion"
      >
        {busy ? "Checking file…" : "Choose Genos update file…"}
      </button>
      {problem && (
        <span role="alert" className="error">
          {problem}
        </span>
      )}
    </div>
  );
}
