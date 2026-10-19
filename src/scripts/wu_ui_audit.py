"""Windows-Use driven UI audit for GenosStyleStudio (desktop app)."""
from __future__ import annotations

import json
import sys
import time
from pathlib import Path

from windows_use.agent.desktop.service import Desktop

OUT = Path(__file__).resolve().parents[2] / "docs" / "development" / "ui-audit-wu.json"


def summarize_state(desktop: Desktop) -> dict:
    state = desktop.get_state(as_bytes=False)
    nodes = []
    if state.tree_state and state.tree_state.interactive_nodes:
        for n in state.tree_state.interactive_nodes[:150]:
            center = getattr(n, "center", None)
            box = getattr(n, "bounding_box", None)
            nodes.append(
                {
                    "name": n.name,
                    "control": n.control_type,
                    "window": getattr(n, "window_name", None),
                    "center": center.to_string() if center else None,
                    "box": (
                        f"{box.left},{box.top},{box.right},{box.bottom}"
                        if box
                        else None
                    ),
                }
            )
    active = state.active_window
    return {
        "active": {
            "title": getattr(active, "name", None) or getattr(active, "title", None)
            if active
            else None,
        },
        "interactive_count": len(nodes),
        "interactive": nodes,
        "scrollable": [
            {
                "name": n.name,
                "control": n.control_type,
                "center": n.center.to_string() if n.center else None,
            }
            for n in (state.tree_state.scrollable_nodes or [])[:40]
        ]
        if state.tree_state
        else [],
        "tree_text": state.tree_state.interactive_elements_to_string()
        if state.tree_state
        else "",
    }


def parse_center(center: str | None) -> tuple[int, int] | None:
    if not center:
        return None
    nums = [int(x) for x in __import__("re").findall(r"-?\d+", center)]
    if len(nums) >= 2:
        return nums[0], nums[1]
    return None


def click_named(desktop: Desktop, nodes: list[dict], *needles: str) -> str | None:
    for n in nodes:
        name = (n["name"] or "").lower()
        for needle in needles:
            if needle.lower() in name:
                pt = parse_center(n["center"])
                if not pt or pt == (0, 0):
                    continue
                desktop.click([pt[0], pt[1]], "left", 1)
                return n["name"]
    return None


def main() -> int:
    desktop = Desktop(use_vision=False, use_annotation=False, use_accessibility=True)
    desktop.warm_up()
    # app(switch) needs a prior get_state() so desktop_state is populated
    desktop.get_state(as_bytes=False)
    print(desktop.app("switch", "GenosStyleStudio", None, None))
    time.sleep(1)

    before = summarize_state(desktop)
    print(f"interactive elements: {before['interactive_count']}")
    sys.stdout.buffer.write((before["tree_text"][:4000] + "\n").encode("utf-8", "replace"))

    # Prefer recent Style open button (not the Remove control).
    clicked = None
    for n in before["interactive"]:
        name = n["name"] or ""
        if name.endswith(" Style") and ".STY" in name and "Remove" not in name:
            pt = parse_center(n["center"])
            if pt:
                desktop.click([pt[0], pt[1]], "left", 1)
                clicked = name
                break
    if not clicked:
        clicked = click_named(desktop, before["interactive"], "Open Yamaha Style")
    if not clicked:
        clicked = click_named(desktop, before["interactive"], "Open")
    print(f"clicked: {clicked}")
    time.sleep(3.0)

    after = summarize_state(desktop)
    print(f"after interactive: {after['interactive_count']}")
    sys.stdout.buffer.write((after["tree_text"][:5000] + "\n").encode("utf-8", "replace"))

    payload = {
        "before": before,
        "after": after,
        "clicked": clicked,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
