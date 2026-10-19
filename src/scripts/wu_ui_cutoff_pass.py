"""Windows-Use: resize GenosStyleStudio and flag clipped / off-window controls."""
from __future__ import annotations

import json
import sys
import time
from pathlib import Path

from windows_use.agent.desktop.service import Desktop

OUT = Path(__file__).resolve().parents[2] / "docs" / "development" / "ui-cutoff-pass.json"


def window_box(desktop: Desktop) -> tuple[int, int, int, int]:
    state = desktop.get_state(as_bytes=False)
    w = state.active_window
    if not w or not w.bounding_box:
        return 0, 0, 0, 0
    b = w.bounding_box
    return b.left, b.top, b.right, b.bottom


def dump_nodes(desktop: Desktop) -> list[dict]:
    state = desktop.get_state(as_bytes=False)
    nodes = []
    for n in state.tree_state.interactive_nodes or []:
        if (n.window_name or "") != "GenosStyleStudio":
            continue
        box = n.bounding_box
        nodes.append(
            {
                "name": n.name,
                "control": n.control_type,
                "cx": n.center.x if n.center else None,
                "cy": n.center.y if n.center else None,
                "left": box.left if box else None,
                "top": box.top if box else None,
                "right": box.right if box else None,
                "bottom": box.bottom if box else None,
            }
        )
    return nodes


def clipped(nodes: list[dict], L: int, T: int, R: int, B: int, pad: int = 4) -> list[dict]:
    bad = []
    for n in nodes:
        if n["cx"] in (None, 0) and n["cy"] in (None, 0):
            continue
        issues = []
        if n["right"] is not None and n["right"] > R - pad:
            issues.append("right-overflow")
        if n["left"] is not None and n["left"] < L + pad:
            # titlebar system menu / left tracks often OK near left
            if n["left"] < L - 2:
                issues.append("left-overflow")
        if n["bottom"] is not None and n["bottom"] > B - pad:
            issues.append("bottom-overflow")
        if n["top"] is not None and n["top"] < T + 20 and n["control"] not in (
            "Button",
            "Menu Item",
            "TitleBar",
        ):
            pass
        if n["bottom"] is not None and n["top"] is not None:
            h = n["bottom"] - n["top"]
            if 0 < h < 6 and n["control"] == "Slider":
                issues.append("tiny-hit-target")
        if issues:
            bad.append({**n, "issues": issues})
    return bad


def click_name(desktop: Desktop, nodes: list[dict], needle: str) -> bool:
    for n in nodes:
        if needle.lower() in (n["name"] or "").lower():
            if n["cx"] and n["cy"]:
                desktop.click([n["cx"], n["cy"]], "left", 1)
                return True
    return False


def main() -> int:
    desktop = Desktop(use_vision=False, use_annotation=False, use_accessibility=True)
    desktop.warm_up()
    desktop.get_state(as_bytes=False)
    print(desktop.app("switch", "GenosStyleStudio", None, None))
    time.sleep(0.3)

    # Narrow layout (near Tauri minWidth 1000)
    print(desktop.app("resize", None, [80, 40], [1100, 720]))
    time.sleep(0.8)

    report: dict = {"sizes": []}

    for label, size in (("narrow", [1100, 720]), ("wide", [1480, 940])):
        print(desktop.app("resize", None, [80, 40], size))
        time.sleep(0.7)
        L, T, R, B = window_box(desktop)
        nodes = dump_nodes(desktop)
        bad = clipped(nodes, L, T, R, B)
        entry = {
            "label": label,
            "window": {"L": L, "T": T, "R": R, "B": B},
            "node_count": len(nodes),
            "clipped_count": len(bad),
            "clipped": bad[:80],
            "names_sample": [n["name"] for n in nodes[:40]],
        }
        report["sizes"].append(entry)
        print(f"{label}: nodes={len(nodes)} clipped={len(bad)} win={L},{T},{R},{B}")
        for c in bad[:25]:
            print(f"  ! {c['issues']} {c['control']}: {c['name']} y={c['cy']} bottom={c['bottom']}")

    # Modal pass at wide size
    print(desktop.app("resize", None, [80, 40], [1480, 940]))
    time.sleep(0.5)
    nodes = dump_nodes(desktop)
    modals = {}
    for label, needle in (
        ("help", "Help"),
        ("limits", "Feature inventory"),
        ("devices", "MIDI device settings"),
        ("library", "Library"),
    ):
        nodes = dump_nodes(desktop)
        if click_name(desktop, nodes, needle):
            time.sleep(1.0)
            L, T, R, B = window_box(desktop)
            mnodes = dump_nodes(desktop)
            # Also include dialog nodes without Genos window name
            state = desktop.get_state(as_bytes=False)
            all_nodes = []
            for n in state.tree_state.interactive_nodes or []:
                box = n.bounding_box
                all_nodes.append(
                    {
                        "name": n.name,
                        "control": n.control_type,
                        "window": n.window_name,
                        "cx": n.center.x if n.center else None,
                        "cy": n.center.y if n.center else None,
                        "bottom": box.bottom if box else None,
                        "right": box.right if box else None,
                    }
                )
            modals[label] = {
                "opened_via": needle,
                "count": len(all_nodes),
                "clipped": clipped(
                    [
                        {
                            **x,
                            "left": None,
                            "top": None,
                            "right": x.get("right"),
                            "bottom": x.get("bottom"),
                        }
                        for x in all_nodes
                        if x.get("window") == "GenosStyleStudio"
                        or "dialog" in (x.get("control") or "").lower()
                        or x.get("name")
                    ],
                    L,
                    T,
                    R,
                    B,
                )[:40],
                "sample": [
                    f"{x['control']}:{x['name']}" for x in all_nodes[:30]
                ],
            }
            desktop.shortcut("escape")
            time.sleep(0.4)
            print(f"modal {label}: elements={modals[label]['count']}")
        else:
            modals[label] = {"error": f"could not click {needle}"}
            print(f"modal {label}: miss")

    report["modals"] = modals
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
