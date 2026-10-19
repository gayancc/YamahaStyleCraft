"""Windows-Use: open Mixer / Tools panels and dump interactive tree."""
from __future__ import annotations

import json
import sys
import time
from pathlib import Path

from windows_use.agent.desktop.service import Desktop

OUT = Path(__file__).resolve().parents[2] / "docs" / "development" / "ui-audit-panels.json"


def dump(desktop: Desktop) -> dict:
    state = desktop.get_state(as_bytes=False)
    nodes = []
    for n in (state.tree_state.interactive_nodes or [])[:200]:
        nodes.append(
            {
                "name": n.name,
                "control": n.control_type,
                "center": n.center.to_string() if n.center else None,
                "box": n.bounding_box.xywh_to_string() if n.bounding_box else None,
            }
        )
    return {
        "count": len(nodes),
        "nodes": nodes,
        "tree": state.tree_state.interactive_elements_to_string()
        if state.tree_state
        else "",
    }


def click_by_name(desktop: Desktop, nodes: list[dict], needle: str) -> bool:
    for n in nodes:
        if needle.lower() in (n["name"] or "").lower():
            nums = [int(x) for x in __import__("re").findall(r"-?\d+", n["center"] or "")]
            if len(nums) >= 2 and nums[0] and nums[1]:
                desktop.click([nums[0], nums[1]], "left", 1)
                return True
    return False


def main() -> int:
    desktop = Desktop(use_vision=False, use_annotation=False, use_accessibility=True)
    desktop.warm_up()
    desktop.get_state(as_bytes=False)
    print(desktop.app("switch", "GenosStyleStudio", None, None))
    time.sleep(0.5)
    base = dump(desktop)
    click_by_name(desktop, base["nodes"], "Mixer")
    time.sleep(1.5)
    mixer = dump(desktop)
    click_by_name(desktop, mixer["nodes"], "Tools")
    time.sleep(0.8)
    tools = dump(desktop)
    payload = {"mixer": mixer, "tools": tools}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    sys.stdout.buffer.write(
        ("MIXER\n" + mixer["tree"][:3500] + "\n\nTOOLS\n" + tools["tree"][:3500] + "\n").encode(
            "utf-8", "replace"
        )
    )
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
