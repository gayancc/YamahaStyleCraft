"""Force GenosStyleStudio foreground and report clipped interactive controls."""
from __future__ import annotations

import json
import os
import time
import ctypes
from ctypes import wintypes
from pathlib import Path

os.environ["ANONYMIZED_TELEMETRY"] = "false"

from windows_use.agent.desktop.service import Desktop

OUT = Path(__file__).resolve().parents[2] / "docs" / "development" / "wu-verify-layout.json"
user32 = ctypes.windll.user32
SW_RESTORE = 9


def find_genos() -> int | None:
    found: list[int] = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    def enum_proc(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            buf = ctypes.create_unicode_buffer(512)
            user32.GetWindowTextW(hwnd, buf, 512)
            if buf.value == "GenosStyleStudio":
                found.append(hwnd)
        return True

    user32.EnumWindows(enum_proc, 0)
    return found[0] if found else None


def genos_box(state) -> tuple[int, int, int, int] | None:
    for win in state.windows or []:
        if win.name == "GenosStyleStudio" and win.bounding_box and win.bounding_box.width > 100:
            b = win.bounding_box
            return b.left, b.top, b.right, b.bottom
    if state.active_window and state.active_window.name == "GenosStyleStudio":
        b = state.active_window.bounding_box
        if b:
            return b.left, b.top, b.right, b.bottom
    return None


def clipped_nodes(state, box: tuple[int, int, int, int]) -> list[dict]:
    L, T, R, B = box
    bad: list[dict] = []
    for n in state.tree_state.interactive_nodes or []:
        if (n.window_name or "") != "GenosStyleStudio":
            continue
        if n.name in ("Close", "Maximize", "Minimize", "System"):
            continue
        bb = n.bounding_box
        if not bb:
            continue
        issues = []
        if bb.bottom > B - 3:
            issues.append("bottom")
        if bb.right > R - 3:
            issues.append("right")
        if bb.left < L - 2:
            issues.append("left")
        if issues:
            bad.append(
                {
                    "name": n.name,
                    "control": n.control_type,
                    "top": bb.top,
                    "bottom": bb.bottom,
                    "left": bb.left,
                    "right": bb.right,
                    "issues": issues,
                }
            )
    return bad


def click_name(desktop: Desktop, state, needle: str) -> bool:
    for n in state.tree_state.interactive_nodes or []:
        if (n.window_name or "") != "GenosStyleStudio":
            continue
        if n.name and needle.lower() in n.name.lower() and n.center:
            desktop.click([n.center.x, n.center.y], "left", 1)
            return True
    return False


def main() -> int:
    hwnd = find_genos()
    if not hwnd:
        print("GenosStyleStudio not found")
        return 1
    user32.ShowWindow(hwnd, SW_RESTORE)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.3)

    desktop = Desktop(use_vision=False, use_annotation=False, use_accessibility=True)
    desktop.warm_up()
    report: dict = {"sizes": [], "modals": {}}

    for label, size in (("narrow", (1100, 720)), ("wide", (1480, 940))):
        user32.MoveWindow(hwnd, 80, 40, size[0], size[1], True)
        time.sleep(0.7)
        user32.SetForegroundWindow(hwnd)
        time.sleep(0.2)
        # focus webview
        desktop.click([200, 90], "left", 1)
        time.sleep(0.25)
        state = desktop.get_state(as_bytes=False)
        box = genos_box(state)
        if not box:
            report["sizes"].append({"label": label, "error": "no window box"})
            print(label, "NO BOX")
            continue
        bad = clipped_nodes(state, box)
        # chrome height proxy: first section chip or SOURCE
        chrome = None
        for n in state.tree_state.interactive_nodes or []:
            if (n.window_name or "") != "GenosStyleStudio" or not n.bounding_box:
                continue
            nm = n.name or ""
            if nm.startswith("Main ") or nm.startswith("Intro ") or "SOURCE" in nm:
                chrome = n.bounding_box.top - box[1]
                break
        entry = {
            "label": label,
            "window": {"L": box[0], "T": box[1], "R": box[2], "B": box[3]},
            "clipped_count": len(bad),
            "clipped": bad[:40],
            "chrome_to_content_px": chrome,
        }
        report["sizes"].append(entry)
        print(f"{label}: clipped={len(bad)} chrome~={chrome} win={box}")
        for c in bad[:15]:
            print(f"  ! {c['issues']} {c['control']}: {c['name']}")

    # Modal smoke at wide
    user32.MoveWindow(hwnd, 80, 40, 1480, 940, True)
    time.sleep(0.5)
    user32.SetForegroundWindow(hwnd)
    for key, needle in (
        ("recording", "Recording & MIDI"),
        ("help", "Help"),
        ("devices", "MIDI device"),
        ("library", "Library"),
    ):
        state = desktop.get_state(as_bytes=False)
        desktop.shortcut("escape")
        time.sleep(0.25)
        state = desktop.get_state(as_bytes=False)
        ok = click_name(desktop, state, needle)
        time.sleep(0.7)
        state = desktop.get_state(as_bytes=False)
        box = genos_box(state)
        names = [
            n.name
            for n in (state.tree_state.interactive_nodes or [])
            if (n.window_name or "") == "GenosStyleStudio" and n.name
        ]
        markers = [
            n
            for n in names
            if any(
                k in n.lower()
                for k in ("done", "close", "cancel", "apply", "import", "ok", "midi thru", "search")
            )
        ]
        bad = clipped_nodes(state, box) if box else []
        report["modals"][key] = {
            "opened": ok,
            "markers": markers[:20],
            "clipped": bad[:15],
        }
        print(f"modal {key}: opened={ok} markers={markers[:8]} clipped={len(bad)}")
        desktop.shortcut("escape")
        time.sleep(0.3)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
