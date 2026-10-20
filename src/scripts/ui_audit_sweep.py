"""Desktop surface sweep: opens each inventoried dialog in the real window via accessibility Invoke,
captures it with PrintWindow, and dismisses it with Escape. Usage: ui_audit_sweep.py <label> [only,names]"""
from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(__import__("pathlib").Path(__file__).parent))
import ui_audit_uia as u  # noqa: E402

SURFACES = [
    ("copy-material", ["Copy"]),
    ("section-length", ["Length"]),
    ("library", ["Library"]),
    ("voice-picker", ["Voice"]),
    ("effect-types", ["Effect types"]),
    ("midi-exchange", ["MIDI exchange"]),
    ("chord-preview", ["Chord preview"]),
    ("piano-tools", ["Piano tools"]),
    ("expansion-recovery", ["Studio", "Expansion Pack Recovery"]),
    ("feature-inventory", ["Studio", "Feature inventory / limits"]),
    ("recording-midi", ["Studio", "Recording & MIDI settings"]),
    ("help", ["Studio", "Help"]),
    ("midi-device-settings", ["Studio", "MIDI device settings"]),
    ("new-blank-style", ["File", "New"]),
    ("diagnostics", ["Structure validated"]),
    ("add-section", ["Add Ending 2 (not in this Style)"]),
    ("source-tracks", ["CASM", "Edit source tracks"]),
]


def esc(times: int = 2) -> None:
    for _ in range(times):
        u.window().SetActive()
        u.uia.SendKeys("{Esc}")
        time.sleep(0.35)


def measure() -> dict:
    """Contrast and layout of what is on screen, read from the webview over its debug port (read-only)."""
    out = subprocess.run(
        ["node", "scripts/ui-audit-measure.mjs"],
        cwd=str(Path(__file__).resolve().parents[1]),
        capture_output=True,
        text=True,
        timeout=120,
    )
    lines = [ln for ln in out.stdout.splitlines() if ln.startswith("{")]
    return json.loads(lines[-1]) if lines else {"error": out.stderr[-300:]}


def main() -> None:
    label = sys.argv[1] if len(sys.argv) > 1 else "run"
    only = set(sys.argv[2].split(",")) if len(sys.argv) > 2 else None
    results: list[dict] = []
    for name, steps in SURFACES:
        if only and name not in only:
            continue
        try:
            for s in steps:
                u.click(s)
            time.sleep(0.8)
            shot = u.capture(f"dsk-{label}-{name}.png").name
            m = measure()
            results.append({"surface": name, "shot": shot, **m})
            print(
                f"{name:22s} text={m.get('checked')} contrast_fail={len(m.get('contrast', []))} "
                f"layout={len(m.get('layout', []))} {m.get('error', '')}"
            )
        except SystemExit as e:
            print("NOT OPENED", name, e)
            results.append({"surface": name, "error": f"not opened: {e}"})
        esc()
    (u.OUT / f"dsk-{label}-sweep.json").write_text(json.dumps(results, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
