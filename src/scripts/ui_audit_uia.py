"""Desktop-only UI Automation driver for the FL compliance audit.

Runs the real Genos Style Studio executable (frontend embedded; no dev server, no browser) and drives it
with the repo's vendored Windows UI Automation layer. Captures use Win32 PrintWindow so another window
covering Genos cannot contaminate a screenshot.

    .venv-wu\\Scripts\\python.exe -I src\\scripts\\ui_audit_uia.py <command> [args]

Commands: launch | close | place W H | tree [depth] | click NAME | shot FILE | open-fixture
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import json
import subprocess
import sys
import time
import types
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXE = ROOT / "src" / "target" / "release" / "genos-style-studio.exe"
OUT = ROOT / "docs" / "development" / "fl-compliance" / "rea-2026-10-20"
FIXTURE = ROOT / "docs" / "research" / "experiments" / "fixtures" / "08.3-4-ACOUSTIC-control.STY"

# Load the reviewed local UIA layer without importing any agent/provider code.
_pkg = types.ModuleType("windows_use")
_pkg.__path__ = [str(ROOT / "docs/development/windows-use/tool/windows_use")]
sys.modules["windows_use"] = _pkg
import windows_use.uia as uia  # noqa: E402
from PIL import Image  # noqa: E402

_u = ctypes.WinDLL("user32", use_last_error=True)
_g = ctypes.WinDLL("gdi32", use_last_error=True)
_V = ctypes.c_void_p
_u.GetWindowDC.argtypes = [_V]
_u.GetWindowDC.restype = _V
_u.ReleaseDC.argtypes = [_V, _V]
_u.PrintWindow.argtypes = [_V, _V, wt.UINT]
_u.GetWindowRect.argtypes = [_V, ctypes.POINTER(wt.RECT)]
_g.CreateCompatibleDC.argtypes = [_V]
_g.CreateCompatibleDC.restype = _V
_g.CreateCompatibleBitmap.argtypes = [_V, ctypes.c_int, ctypes.c_int]
_g.CreateCompatibleBitmap.restype = _V
_g.SelectObject.argtypes = [_V, _V]
_g.SelectObject.restype = _V
_g.GetDIBits.argtypes = [_V, _V, wt.UINT, wt.UINT, _V, _V, wt.UINT]
_g.DeleteObject.argtypes = [_V]
_g.DeleteDC.argtypes = [_V]


class _BIH(ctypes.Structure):
    _fields_ = [("biSize", wt.DWORD), ("biWidth", wt.LONG), ("biHeight", wt.LONG), ("biPlanes", wt.WORD),
                ("biBitCount", wt.WORD), ("biCompression", wt.DWORD), ("biSizeImage", wt.DWORD),
                ("biXPelsPerMeter", wt.LONG), ("biYPelsPerMeter", wt.LONG), ("biClrUsed", wt.DWORD),
                ("biClrImportant", wt.DWORD)]


def window():
    for c in uia.GetRootControl().GetChildren():
        if c.Name == "GenosStyleStudio" and c.ClassName == "Tauri Window":
            return c
    raise SystemExit("Genos window not found - run: launch")


def pid() -> int:
    return window().ProcessId


def launch(cdp: bool = False) -> None:
    """Start the embedded-frontend exe. cdp=True exposes the WebView2 debug port for read-only DOM measurements."""
    import os
    env = dict(os.environ)
    if cdp:
        env["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = "--remote-debugging-port=9449"
        env["WEBVIEW2_USER_DATA_FOLDER"] = str(Path(env.get("TEMP", ".")) / "genos-ui-audit-wv2")
    subprocess.Popen([str(EXE)], env=env, cwd=str(EXE.parent))
    for _ in range(60):
        time.sleep(0.5)
        try:
            window()
            time.sleep(1.5)
            return
        except SystemExit:
            continue
    raise SystemExit("Genos did not open a window")


def close() -> None:
    subprocess.run(["taskkill", "/IM", "genos-style-studio.exe", "/F"], capture_output=True)


def place(w: int = 1480, h: int = 940) -> None:
    win = window()
    hwnd = win.NativeWindowHandle
    _u.ShowWindow(hwnd, 9)
    time.sleep(0.4)
    _u.MoveWindow(hwnd, 20, 10, w, h, True)
    time.sleep(0.8)


def capture(name: str) -> Path:
    win = window()
    hwnd = win.NativeWindowHandle
    r = wt.RECT()
    _u.GetWindowRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    hdc = _u.GetWindowDC(hwnd)
    mdc = _g.CreateCompatibleDC(hdc)
    bmp = _g.CreateCompatibleBitmap(hdc, w, h)
    _g.SelectObject(mdc, bmp)
    _u.PrintWindow(hwnd, mdc, 2)
    bi = _BIH(ctypes.sizeof(_BIH), w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
    buf = ctypes.create_string_buffer(w * h * 4)
    _g.GetDIBits(mdc, bmp, 0, h, buf, ctypes.byref(bi), 0)
    _g.DeleteObject(bmp)
    _g.DeleteDC(mdc)
    _u.ReleaseDC(hwnd, hdc)
    path = OUT / name
    Image.frombuffer("RGBA", (w, h), buf, "raw", "BGRA", 0, 1).convert("RGB").save(path)
    return path


def image_to_screen(x: float, y: float) -> tuple[int, int]:
    """Screen position of a point in a PrintWindow capture (the image origin is the window's top-left)."""
    r = wt.RECT()
    _u.GetWindowRect(window().NativeWindowHandle, ctypes.byref(r))
    return int(r.left + x), int(r.top + y)


def hold(x: float, y: float, name: str, release_outside: bool = True) -> Path:
    """Hover then press-and-hold the real mouse at a captured-image coordinate, capture, then release.
    Releasing outside the button cancels the click, so the held (lit) state is captured without side effects."""
    sx, sy = image_to_screen(x, y)
    _u.SetCursorPos(sx, sy)
    time.sleep(0.35)
    _u.mouse_event(2, 0, 0, 0, 0)
    time.sleep(0.25)
    path = capture(name)
    if release_outside:
        _u.SetCursorPos(sx, sy + 400)
        time.sleep(0.15)
    _u.mouse_event(4, 0, 0, 0, 0)
    return path


def click_at(x: float, y: float) -> None:
    """Real mouse click at a captured-image coordinate."""
    sx, sy = image_to_screen(x, y)
    _u.SetCursorPos(sx, sy)
    time.sleep(0.25)
    _u.mouse_event(2, 0, 0, 0, 0)
    time.sleep(0.08)
    _u.mouse_event(4, 0, 0, 0, 0)
    time.sleep(0.6)


def hover_at(x: float, y: float, name: str) -> Path:
    sx, sy = image_to_screen(x, y)
    _u.SetCursorPos(sx, sy)
    time.sleep(0.6)
    return capture(name)


def find(name: str, depth: int = 25, ctrl: str | None = None):
    kw = {"Name": name, "searchDepth": depth}
    c = window().Control(**kw) if ctrl is None else getattr(window(), ctrl)(**kw)
    if not c.Exists(2, 0.2):
        raise SystemExit(f"control not found: {name!r}")
    return c


def click(name: str, ctrl: str | None = None) -> None:
    """Activate a control through its accessibility Invoke action (position independent).

    WebView content coordinates reported by UIA can be offset from the host window, so a coordinate click
    may hit a different element. Buttons are looked up as typed ButtonControls, which expose Invoke; a
    coordinate click is only the fallback for other control types."""
    button = window().ButtonControl(Name=name, searchDepth=30)
    if ctrl is None and button.Exists(2, 0.2):
        # <summary> menu buttons expose no Invoke pattern; Chromium maps their action to ExpandCollapse
        # or the legacy default action.
        for getter, action in (
            ("GetSelectionItemPattern", "Select"),   # tabs
            ("GetInvokePattern", "Invoke"),
            ("GetExpandCollapsePattern", "Expand"),
            ("GetLegacyIAccessiblePattern", "DoDefaultAction"),
        ):
            pattern = getattr(button, getter, lambda: None)()
            if pattern is not None:
                getattr(pattern, action)()
                time.sleep(0.5)
                return
    find(name, ctrl=ctrl).Click(simulateMove=False)
    time.sleep(0.5)


def tree(depth: int = 14, limit: int = 400):
    rows = []

    def walk(c, d):
        if d > depth or len(rows) >= limit:
            return
        for ch in c.GetChildren():
            r = ch.BoundingRectangle
            rows.append({"d": d, "type": ch.ControlTypeName, "name": ch.Name, "cls": ch.ClassName,
                         "rect": [r.left, r.top, r.right, r.bottom]})
            walk(ch, d + 1)

    walk(window(), 0)
    return rows


def open_fixture(path: Path = FIXTURE) -> None:
    """Open the fixture through the real native file chooser (empty-state button, else File > Open).

    The chooser is an owned dialog, which UIA exposes as a child of the Tauri window."""
    try:
        click("Open Yamaha Style")
    except SystemExit:
        click("File")
        click("Open")
    dlg = None
    for _ in range(40):
        time.sleep(0.25)
        for c in list(window().GetChildren()) + list(uia.GetRootControl().GetChildren()):
            if c.ClassName == "#32770" and c.ProcessId == pid():
                dlg = c
                break
        if dlg:
            break
    if not dlg:
        raise SystemExit("file dialog did not open")
    edit = dlg.EditControl(Name="File name:", searchDepth=10)
    if not edit.Exists(3):
        raise SystemExit("file name edit not found")
    edit.SetFocus()
    time.sleep(0.2)
    uia.SendKeys("{Ctrl}a", waitTime=0.1)
    # SendKeys treats {}^%+~ as syntax; escape them so a path is typed literally.
    literal = "".join("{" + ch + "}" if ch in "{}^%+~()" else ch for ch in str(path))
    uia.SendKeys(literal, interval=0.01, waitTime=0.2)
    typed = edit.GetValuePattern().Value
    if Path(typed) != Path(path):
        raise SystemExit(f"file name was not typed: {typed!r}")
    uia.SendKeys("{Enter}", waitTime=0.2)
    time.sleep(2.0)


if __name__ == "__main__":
    cmd, *a = sys.argv[1:] or ["tree"]
    if cmd == "launch":
        launch(cdp="--cdp" in a)
    elif cmd == "close":
        close()
    elif cmd == "place":
        place(*(int(x) for x in a[:2]))
    elif cmd == "tree":
        print(json.dumps(tree(int(a[0]) if a else 14), indent=0)[:20000])
    elif cmd == "click":
        click(a[0])
    elif cmd == "shot":
        print(capture(a[0]))
    elif cmd == "open-fixture":
        open_fixture(Path(a[0]) if a else FIXTURE)
    else:
        raise SystemExit(__doc__)
