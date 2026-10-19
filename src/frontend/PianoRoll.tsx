import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useStudio } from "./store";
import type { Note } from "./contracts";
import { ControllerLane } from "./ControllerLane";
import { PianoTools } from "./PianoTools";
import { drawPianoKeyboard, isBlackKey, pianoKeyAt } from "./pianoKeyboard";
import {
  clamp,
  meterAt,
  moveBounds,
  noteInput,
  tickSnap,
  segmentHitsRect,
  validateDraft,
} from "./pianoEditing";
import { DEFAULT_NOTE_COLORS, noteFill, sanitizeNoteColors } from "./noteColors";

const KEY = 60,
  RULER = 28;
const names = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
export const pitchName = (n: number) =>
  `${names[n % 12]}${Math.floor(n / 12) - 1}`;
interface Gesture {
  mode:
    | "move"
    | "resize"
    | "left"
    | "marquee"
    | "draw"
    | "paint"
    | "erase"
    | "pan"
    | "holdRight";
  x: number;
  y: number;
  dx: number;
  dy: number;
  ids: number[];
  note?: Note;
  add?: import("./contracts").NoteInput[];
  remove?: number[];
}

type NotePropsPanel = { x: number; y: number; id: number };
export function PianoRoll({
  disabled = false,
  onAudition,
  playheadTick = null,
  onParkCursor,
  noteColors,
  onNoteColorsChange,
}: {
  disabled?: boolean;
  onAudition?: (pitch: number, velocity: number) => void;
  playheadTick?: number | null;
  /** SFM-PR-TOOL-CURSOR: park yellow play cursor at click tick. */
  onParkCursor?: (tick: number) => void;
  /** Per-channel 1–16 display colors (SFM Note colors). */
  noteColors?: string[];
  onNoteColorsChange?: (colors: string[]) => void;
}) {
  const {
    view,
    sectionId,
    channel,
    selected,
    snap,
    zoom,
    tool,
    busy,
    select,
    edit,
  } = useStudio();
  const channelColors = useMemo(
    () => sanitizeNoteColors(noteColors ?? DEFAULT_NOTE_COLORS),
    [noteColors],
  );
  const partColor = channelColors[Math.max(0, Math.min(15, channel - 1))]!;
  const section = view?.sections.find((s) => s.id === sectionId);
  const scroll = useRef<HTMLDivElement>(null),
    canvas = useRef<HTMLCanvasElement>(null);
  const [ROW, setRow] = useState(18);
  const [length, setLength] = useState(0);
  const [ghosts, setGhosts] = useState(false);
  const [follow, setFollow] = useState(false);
  const [size, setSize] = useState({ w: 800, h: 500 });
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [gesture, setGesture] = useState<Gesture | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: number } | null>(
    null,
  );
  const [noteProps, setNoteProps] = useState<NotePropsPanel | null>(null);
  const cursor = useRef(0);
  const anchor = useRef<Gesture | null>(null);
  const holdRightTimer = useRef<number | null>(null);
  const clearHoldRight = useCallback(() => {
    if (holdRightTimer.current !== null) {
      window.clearTimeout(holdRightTimer.current);
      holdRightTimer.current = null;
    }
  }, []);
  const scale = (72 * zoom) / (view?.ppq ?? 480);
  const grid = ((view?.ppq ?? 480) * 4) / (snap || 16);
  const notes = useMemo(
    () =>
      view?.notes.filter(
        (n) =>
          n.channel === channel &&
          section &&
          n.start >= section.start &&
          n.start < section.end,
      ) ?? [],
    [view, channel, section],
  );
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const gestureSet = useMemo(() => new Set(gesture?.ids ?? []), [gesture]);
  useEffect(() => {
    if (!follow || playheadTick === null || !section || !scroll.current) return;
    const el = scroll.current,
      x = (playheadTick - section.start) * scale;
    if (x < el.scrollLeft || x > el.scrollLeft + el.clientWidth - KEY - 30)
      el.scrollLeft = Math.max(0, x - 40);
  }, [follow, playheadTick, section, scale]);
  useEffect(() => {
    const el = scroll.current;
    if (!el) return;
    const ob = new ResizeObserver(([e]) =>
      setSize({ w: e.contentRect.width, h: e.contentRect.height }),
    );
    ob.observe(el);
    el.scrollTop = (127 - 75) * ROW;
    return () => ob.disconnect();
  }, []);
  useEffect(() => {
    setMenu(null);
    setNoteProps(null);
    setGesture(null);
    anchor.current = null;
    clearHoldRight();
  }, [sectionId, channel, view, disabled, clearHoldRight]);
  useEffect(() => () => clearHoldRight(), [clearHoldRight]);
  useEffect(() => {
    const el = scroll.current;
    if (!el || !view || !section) return;
    const wheel = (e: WheelEvent) => {
      if (e.ctrlKey) {
        e.preventDefault();
        const x = e.clientX - el.getBoundingClientRect().left,
          at = (el.scrollLeft + x - KEY) / scale,
          next = clamp(zoom * (e.deltaY < 0 ? 1.15 : 1 / 1.15), 0.125, 8);
        useStudio.setState({ zoom: next });
        requestAnimationFrame(() => {
          el.scrollLeft = Math.max(0, (at * 72 * next) / view.ppq - x + KEY);
        });
      } else if (e.shiftKey) {
        e.preventDefault();
        el.scrollLeft += e.deltaY;
      } else if (e.altKey && !busy && !disabled && selected.length) {
        e.preventDefault();
        void edit({
          type: "pianoNotes",
          section: section.id,
          channel,
          updates: notes
            .filter((n) => selectedSet.has(n.id))
            .map((n) => ({
              id: n.id,
              start: n.start,
              duration: n.duration,
              pitch: n.pitch,
              velocity: clamp(n.velocity + (e.deltaY < 0 ? 1 : -1), 1, 127),
            })),
        });
      }
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  }, [
    view,
    section,
    scale,
    zoom,
    busy,
    disabled,
    selected,
    selectedSet,
    notes,
    channel,
    edit,
  ]);
  useEffect(() => {
    const el = scroll.current;
    if (!el) return;
    const current = useStudio.getState();
    const sec = current.view?.sections.find((s) => s.id === current.sectionId);
    const ns =
      current.view?.notes.filter(
        (n) =>
          n.channel === current.channel &&
          sec &&
          n.start >= sec.start &&
          n.start < sec.end,
      ) ?? [];
    const center = ns.length
      ? ns.reduce((sum, n) => sum + n.pitch, 0) / ns.length
      : 60;
    el.scrollTop = Math.max(
      0,
      (127 - center) * ROW - el.clientHeight / 2 + RULER,
    );
    el.scrollLeft = 0;
  }, [sectionId, channel]);
  const rect = useCallback(
    (n: Note) => {
      const active = gestureSet.has(n.id);
      const dt = active && gesture?.mode === "move" ? gesture.dx : 0;
      const dp = active && gesture?.mode === "move" ? gesture.dy : 0;
      const dd = active && gesture?.mode === "resize" ? gesture.dx : 0;
      const left = active && gesture?.mode === "left" ? gesture.dx : 0;
      return {
        x:
          KEY +
          (n.start - (section?.start ?? 0) + dt + left) * scale -
          offset.x,
        y: RULER + (127 - n.pitch - dp) * ROW - offset.y,
        w: Math.max(3, (n.duration + dd - left) * scale),
        h: ROW - 2,
      };
    },
    [gesture, gestureSet, section?.start, scale, offset, ROW],
  );
  useEffect(() => {
    if (!canvas.current || !view || !section) return;
    const dpr = window.devicePixelRatio || 1;
    const c = canvas.current;
    c.width = size.w * dpr;
    c.height = size.h * dpr;
    c.style.width = `${size.w}px`;
    c.style.height = `${size.h}px`;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.fillStyle = "#191c1f";
    ctx.fillRect(0, 0, size.w, size.h);
    ctx.font = "10px Segoe UI";
    for (let p = 0; p < 128; p++) {
      const y = RULER + (127 - p) * ROW - offset.y;
      if (y < -ROW || y > size.h) continue;
      const black = isBlackKey(p);
      ctx.fillStyle = black ? "#27363e" : "#2e3e47";
      ctx.fillRect(KEY, y, size.w - KEY, ROW);
      if (p % 12 === 0) {
        ctx.strokeStyle = "#353a3e";
        ctx.beginPath();
        ctx.moveTo(KEY, y + ROW);
        ctx.lineTo(size.w, y + ROW);
        ctx.stroke();
      }
    }
    drawPianoKeyboard(ctx, KEY, size.h, ROW, RULER, offset.y, pitchName);
    const { beat, bar } = meterAt(view, section.start);
    const first = Math.max(0, Math.floor(offset.x / (grid * scale)));
    // Bound grid density at extreme zoom; beat/bar lines still remain readable.
    const stride = Math.max(1, Math.ceil(5 / (grid * scale)));
    for (let i = first; i * grid * scale - offset.x < size.w; i += stride) {
      const tick = i * grid;
      if (tick > section.end - section.start) break;
      const x = KEY + tick * scale - offset.x;
      if (x < KEY) continue;
      ctx.strokeStyle =
        tick % bar === 0
          ? "#444a4e"
          : tick % beat === 0
            ? "#30363a"
            : "#252b2f";
      ctx.beginPath();
      ctx.moveTo(x, RULER);
      ctx.lineTo(x, size.h);
      ctx.stroke();
    }
    // Beat/bar separators do not depend on the fine-grid stride at extreme zoom.
    for (
      let i = Math.max(0, Math.floor(offset.x / (beat * scale)));
      i * beat <= section.end - section.start &&
      KEY + i * beat * scale - offset.x < size.w;
      i++
    ) {
      const x = KEY + i * beat * scale - offset.x;
      if (x < KEY) continue;
      ctx.fillStyle = i % Math.round(bar / beat) === 0 ? "#52616a" : "#3b4b54";
      ctx.fillRect(Math.round(x), RULER, 1, size.h - RULER);
    }
    ctx.save();
    ctx.beginPath();
    ctx.rect(KEY, RULER, size.w - KEY, size.h - RULER);
    ctx.clip();
    if (ghosts)
      for (const n of view.notes.filter(
        (n) =>
          n.channel !== channel &&
          n.start >= section.start &&
          n.start < section.end,
      )) {
        const r = rect(n);
        if (
          r.x + r.w >= KEY &&
          r.x < size.w &&
          r.y + r.h >= RULER &&
          r.y < size.h
        ) {
          ctx.fillStyle = "#839da53b";
          ctx.fillRect(
            Math.max(KEY, r.x),
            Math.max(RULER, r.y + 1),
            Math.max(0, r.w - Math.max(0, KEY - r.x)),
            r.h,
          );
        }
      }
    const removed = new Set(gesture?.remove ?? []);
    const rendered = [
      ...notes.filter((n) => !removed.has(n.id)),
      ...(gesture?.add ?? []).map((n, i) => ({
        ...n,
        id: -1 - i,
        offId: -1 - i,
      })),
    ];
    for (const n of rendered) {
      const r = rect(n);
      if (r.x + r.w < KEY || r.x > size.w || r.y + r.h < RULER || r.y > size.h)
        continue;
      const chosen = selectedSet.has(n.id);
      ctx.fillStyle = noteFill(partColor, n.velocity, chosen);
      if (r.w < 6) ctx.fillRect(r.x, r.y + 1, r.w, r.h);
      else {
        ctx.beginPath();
        ctx.roundRect(r.x, r.y + 1, r.w, r.h, 2);
        ctx.fill();
      }
      if (chosen) {
        ctx.strokeStyle = "#f5b6bd";
        ctx.strokeRect(r.x, r.y + 1, r.w, r.h);
      }
      if (r.w > 40) {
        ctx.fillStyle = "#29251d";
        ctx.fillText(pitchName(n.pitch), r.x + 5, r.y + 12);
      }
    }
    ctx.restore();
    ctx.fillStyle = "#22262a";
    ctx.fillRect(0, 0, size.w, RULER);
    ctx.fillStyle = "#aeb5ba";
    for (
      let i = Math.max(0, Math.floor(offset.x / (bar * scale)));
      i * bar * scale - offset.x < size.w;
      i++
    ) {
      const x = KEY + i * bar * scale - offset.x;
      if (x >= KEY) {
        ctx.fillText(`${i + 1}`, x + 7, 18);
        ctx.fillRect(x, RULER - 5, 1, 5);
      }
    }
    ctx.fillStyle = "#30363a";
    ctx.fillRect(0, 0, KEY, RULER);
    ctx.fillStyle = "#a4acb0";
    ctx.fillText("PITCH", 12, 18);
    if (gesture?.mode === "marquee") {
      ctx.fillStyle = "#e7b96a20";
      ctx.strokeStyle = "#e7b96a";
      ctx.fillRect(gesture.x, gesture.y, gesture.dx, gesture.dy);
      ctx.strokeRect(gesture.x, gesture.y, gesture.dx, gesture.dy);
    }
    if (playheadTick !== null) {
      const x = KEY + (playheadTick - section.start) * scale - offset.x;
      if (x >= KEY && x < size.w) {
        ctx.fillStyle = "#e9c477";
        ctx.fillRect(x, 0, 2, size.h);
      }
    }
  }, [
    view,
    section,
    notes,
    selected,
    gesture,
    size,
    offset,
    scale,
    grid,
    rect,
    ROW,
    ghosts,
    channel,
    partColor,
    playheadTick,
    selectedSet,
  ]);
  const point = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const hit = (x: number, y: number) =>
    [...notes].reverse().find((n) => {
      const r = rect(n);
      return (
        x >= Math.max(KEY, r.x) && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
      );
    });
  const tickAt = (x: number, bypass = false) =>
    Math.max(
      0,
      tickSnap((x - KEY + offset.x) / scale, bypass || !snap ? 1 : grid),
    );
  const paintNote = (x: number, y: number, bypass = false) => {
    if (!section || x < KEY || y < RULER) return null;
    const local = tickAt(x, bypass);
    if (local >= section.end - section.start) return null;
    return {
      track: notes[0]?.track ?? 0,
      channel,
      pitch: clamp(127 - Math.floor((y - RULER + offset.y) / ROW), 0, 127),
      velocity: 90,
      start: section.start + local,
      duration: Math.max(
        1,
        Math.min(
          Math.round(length || grid),
          section.end - section.start - local,
        ),
      ),
    };
  };
  const duplicate = () => {
    if (!section || busy || disabled) return;
    const ns = notes.filter((n) => selectedSet.has(n.id));
    if (!ns.length) return;
    const first = Math.min(...ns.map((n) => n.start)),
      last = Math.max(...ns.map((n) => n.start + n.duration));
    const dt = Math.max(
      1,
      Math.ceil((last - first) / Math.max(1, grid)) * Math.max(1, grid),
    );
    const add = ns.map((n) => ({
      ...noteInput(n),
      start: n.start + Math.round(dt),
    }));
    try {
      validateDraft(add, section);
      void edit({ type: "pianoNotes", section: section.id, channel, add });
    } catch (e) {
      useStudio.setState({ error: String(e) });
    }
  };
  const add = (x: number, y: number) => {
    if (!section || !view || busy || disabled || x < KEY || y < RULER) return;
    const local = Math.min(
      tickAt(x),
      Math.max(0, section.end - section.start - 1),
    );
    cursor.current = local;
    void edit({
      type: "addNotes",
      section: section.id,
      notes: [
        {
          track: notes[0]?.track ?? 0,
          channel,
          pitch: Math.max(
            0,
            Math.min(127, 127 - Math.floor((y - RULER + offset.y) / ROW)),
          ),
          velocity: 90,
          start: section.start + local,
          duration: Math.max(
            1,
            Math.min(
              Math.round(length || grid),
              section.end - section.start - local,
            ),
          ),
        },
      ],
    });
  };
  const down = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (![0, 1, 2].includes(e.button) || busy || disabled) return;
    setMenu(null);
    setNoteProps(null);
    clearHoldRight();
    const { x, y } = point(e);
    const n = hit(x, y);
    if (x < KEY && y >= RULER && e.button === 0) {
      const pitch = pianoKeyAt(x, y - RULER + offset.y, ROW, KEY);
      if (pitch !== null) onAudition?.(pitch, 90);
      return;
    }
    if (!section || x < KEY || y < RULER) return;
    cursor.current = tickAt(x, e.altKey);
    if (e.button === 1 || (e.button === 0 && tool === "pan")) {
      anchor.current = {
        mode: "pan",
        x,
        y,
        dx: offset.x,
        dy: offset.y,
        ids: [],
      };
      setGesture(anchor.current);
      e.currentTarget.setPointerCapture(e.pointerId);
      return;
    }
    if (e.button === 0 && tool === "cursor") {
      const tick = section.start + tickAt(x, e.altKey);
      onParkCursor?.(
        Math.max(section.start, Math.min(section.end - 1, tick)),
      );
      return;
    }
    if (e.button === 2 && !e.shiftKey && n && tool !== "erase") {
      // SFM-060: hold-right opens note properties; quick right-click deletes.
      select([n.id]);
      const clientX = e.clientX;
      const clientY = e.clientY;
      const noteId = n.id;
      anchor.current = {
        mode: "holdRight",
        x,
        y,
        dx: 0,
        dy: 0,
        ids: [noteId],
        note: n,
        remove: [noteId],
      };
      // Live SFM help (V2.0.44): "hold the right button for 1 second to edit its properties".
      holdRightTimer.current = window.setTimeout(() => {
        holdRightTimer.current = null;
        if (anchor.current?.mode !== "holdRight") return;
        anchor.current = null;
        setGesture(null);
        setNoteProps({ x: clientX, y: clientY, id: noteId });
      }, 1000);
    } else if ((e.button === 2 && !e.shiftKey) || tool === "erase") {
      anchor.current = {
        mode: "erase",
        x,
        y,
        dx: 0,
        dy: 0,
        ids: [],
        remove: n ? [n.id] : [],
      };
    } else if (e.button === 2) {
      return;
    } else if (tool === "paint" && !e.ctrlKey) {
      const note = paintNote(x, y, e.altKey);
      anchor.current = {
        mode: "paint",
        x,
        y,
        dx: 0,
        dy: 0,
        ids: [],
        add:
          note &&
          !notes.some((n) => n.start === note.start && n.pitch === note.pitch)
            ? [note]
            : [],
      };
    } else if (!n && tool === "draw" && !e.ctrlKey) {
      const note = paintNote(x, y, e.altKey);
      anchor.current = {
        mode: "draw",
        x,
        y,
        dx: 0,
        dy: 0,
        ids: [],
        add: note ? [note] : [],
      };
    } else if (n) {
      const ids = e.shiftKey
        ? selectedSet.has(n.id)
          ? selected.filter((id) => id !== n.id)
          : [...selected, n.id]
        : selectedSet.has(n.id)
          ? selected
          : [n.id];
      select(ids);
      const r = rect(n);
      setLength(n.duration);
      anchor.current = {
        mode:
          x > r.x + r.w - Math.min(7, r.w / 3)
            ? "resize"
            : x < r.x + Math.min(4, r.w / 3)
              ? "left"
              : "move",
        x,
        y,
        dx: 0,
        dy: 0,
        ids,
        note: n,
      };
    } else {
      if (!e.shiftKey) select([]);
      anchor.current = {
        mode: "marquee",
        x,
        y,
        dx: 0,
        dy: 0,
        ids: e.shiftKey ? selected : [],
      };
    }
    setGesture(anchor.current);
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const move = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const a = anchor.current;
    if (!a) {
      const { x, y } = point(e),
        n = hit(x, y);
      if (n) {
        const r = rect(n);
        e.currentTarget.style.cursor =
          x > r.x + r.w - 7 || x < r.x + 4 ? "ew-resize" : "move";
        e.currentTarget.title = `${pitchName(n.pitch)} · tick ${n.start - (section?.start ?? 0)} · length ${n.duration} · velocity ${n.velocity}`;
      } else {
        e.currentTarget.style.cursor =
          x < KEY
            ? "pointer"
            : tool === "pan"
              ? "grab"
              : tool === "cursor"
                ? "col-resize"
                : tool === "select"
                  ? "default"
                  : "crosshair";
        e.currentTarget.title =
          tool === "cursor"
            ? "Click to park play cursor"
            : tool === "pan"
              ? "Drag to pan"
              : "";
      }
      return;
    }
    const { x, y } = point(e);
    if (a.mode === "pan") {
      if (scroll.current) {
        scroll.current.scrollLeft = a.dx + a.x - x;
        scroll.current.scrollTop = a.dy + a.y - y;
      }
      return;
    }
    if (a.mode === "holdRight") {
      if (Math.hypot(x - a.x, y - a.y) >= 6) {
        clearHoldRight();
        const g: Gesture = {
          mode: "erase",
          x,
          y,
          dx: 0,
          dy: 0,
          ids: [],
          remove: a.remove ?? [],
        };
        anchor.current = g;
        setGesture(g);
      }
      return;
    }
    if (a.mode === "draw") {
      const note = paintNote(x, y, e.altKey);
      const g = { ...a, add: note ? [note] : [] };
      anchor.current = g;
      setGesture(g);
      return;
    }
    if (a.mode === "paint") {
      const adds = [...(a.add ?? [])],
        steps = Math.min(
          5000,
          Math.max(1, Math.ceil(Math.abs(x - a.x) / Math.max(1, grid * scale))),
        );
      for (let i = 1; i <= steps; i++) {
        const note = paintNote(
          a.x + ((x - a.x) * i) / steps,
          a.y + ((y - a.y) * i) / steps,
          e.altKey,
        );
        if (
          note &&
          !notes.some(
            (n) => n.start === note.start && n.pitch === note.pitch,
          ) &&
          !adds.some((n) => n.start === note.start && n.pitch === note.pitch)
        )
          adds.push(note);
      }
      const g = { ...a, x, y, add: adds };
      anchor.current = g;
      setGesture(g);
      return;
    }
    if (a.mode === "erase") {
      const remove = notes
        .filter((n) => {
          const r = rect(n);
          return segmentHitsRect(a.x, a.y, x, y, r);
        })
        .map((n) => n.id);
      const g = {
        ...a,
        x,
        y,
        remove: [...new Set([...(a.remove ?? []), ...remove])],
      };
      anchor.current = g;
      setGesture(g);
      return;
    }
    const dx =
      a.mode === "marquee"
        ? x - a.x
        : tickSnap((x - a.x) / scale, e.altKey || !snap ? 1 : grid);
    let dy = a.mode === "marquee" ? y - a.y : Math.round((a.y - y) / ROW);
    let dt = dx;
    const ns = notes.filter((n) => a.ids.includes(n.id));
    if (a.mode === "move" && section && ns.length) {
      const bounded = moveBounds(ns, section, dt, dy);
      dt = bounded.dt;
      dy = bounded.dp;
    }
    if ((a.mode === "resize" || a.mode === "left") && section && ns.length)
      dt =
        a.mode === "left"
          ? clamp(
              dt,
              Math.max(...ns.map((n) => section.start - n.start)),
              Math.min(...ns.map((n) => n.duration - 1)),
            )
          : clamp(
              dt,
              Math.max(...ns.map((n) => 1 - n.duration)),
              Math.min(...ns.map((n) => section.end - n.start - n.duration)),
            );
    const g = { ...a, dx: dt, dy };
    anchor.current = g;
    setGesture(g);
    if (a.mode === "marquee") {
      const left = Math.min(a.x, x),
        right = Math.max(a.x, x),
        top = Math.min(a.y, y),
        bottom = Math.max(a.y, y);
      select([
        ...new Set([
          ...a.ids,
          ...notes
            .filter((n) => {
              const r = rect(n);
              return (
                r.x < right &&
                r.x + r.w > left &&
                r.y < bottom &&
                r.y + r.h > top
              );
            })
            .map((n) => n.id),
        ]),
      ]);
    }
  };
  const up = () => {
    const g = anchor.current;
    anchor.current = null;
    setGesture(null);
    const wasHolding = holdRightTimer.current !== null;
    clearHoldRight();
    if (!g || !section || busy || disabled) return;
    if (g.mode === "holdRight") {
      // Quick right-click on a note deletes (SFM Smart Tool right-delete).
      if (wasHolding && g.remove?.length)
        void edit({
          type: "pianoNotes",
          section: section.id,
          channel,
          remove: g.remove,
        });
      return;
    }
    if (g.mode === "paint" || g.mode === "draw") {
      if (g.add?.length)
        void edit({
          type: "pianoNotes",
          section: section.id,
          channel,
          add: g.add,
        });
      return;
    }
    if (g.mode === "erase") {
      if (g.remove?.length)
        void edit({
          type: "pianoNotes",
          section: section.id,
          channel,
          remove: g.remove,
        });
      return;
    }
    // Smart Tool (select): empty click inserts a note; drag marquees (SFM-060).
    if (g.mode === "marquee") {
      if (tool === "select" && Math.hypot(g.dx, g.dy) < 4 && !g.ids.length) {
        const note = paintNote(g.x, g.y, false);
        if (note)
          void edit({
            type: "pianoNotes",
            section: section.id,
            channel,
            add: [note],
          });
      }
      return;
    }
    if (!g.note) return;
    if (g.mode === "move" && (g.dx || g.dy))
      void edit({
        type: "updateNotes",
        section: section.id,
        ids: g.ids,
        deltaTick: g.dx,
        deltaPitch: g.dy,
      });
    if (g.mode === "resize" && g.dx)
      void edit({
        type: "updateNotes",
        section: section.id,
        ids: g.ids,
        deltaDuration: g.dx,
      });
    if (g.mode === "left" && g.dx)
      void edit({
        type: "updateNotes",
        section: section.id,
        ids: g.ids,
        deltaTick: g.dx,
        deltaDuration: -g.dx,
      });
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (
        document.querySelector('[aria-modal="true"]') ||
        busy ||
        disabled ||
        (e.target as HTMLElement).closest(".controller-lane")
      )
        return;
      if (
        (e.target as HTMLElement).closest(
          'input,select,textarea,[contenteditable="true"]',
        ) ||
        !section ||
        !view
      )
        return;
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && e.key.toLowerCase() === "q") {
        e.preventDefault();
        const ns = selected.length
          ? notes.filter((n) => selectedSet.has(n.id))
          : notes;
        if (ns.length)
          void edit({
            type: "quantize",
            section: section.id,
            ids: ns.map((n) => n.id),
            step: Math.max(1, Math.round(grid)),
          });
      }
      if (e.key === "Escape") {
        anchor.current = null;
        setGesture(null);
        setMenu(null);
      }
      if (
        !ctrl &&
        !e.altKey &&
        ["p", "b", "e", "d", "2", "3"].includes(e.key.toLowerCase())
      ) {
        const map = {
          p: "draw",
          b: "paint",
          e: "select",
          d: "erase",
          "2": "cursor",
          "3": "pan",
        } as const;
        useStudio.setState({
          tool: map[e.key.toLowerCase() as keyof typeof map],
        });
      }
      if (
        ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key) &&
        selected.length
      ) {
        e.preventDefault();
        const ns = notes.filter((n) => selectedSet.has(n.id));
        const delta = e.altKey ? 1 : Math.max(1, Math.round(grid));
        const { dt, dp } = moveBounds(
          ns,
          section,
          e.key === "ArrowLeft" ? -delta : e.key === "ArrowRight" ? delta : 0,
          e.key === "ArrowUp"
            ? ctrl
              ? 12
              : 1
            : e.key === "ArrowDown"
              ? ctrl
                ? -12
                : -1
              : 0,
        );
        void edit({
          type: "updateNotes",
          section: section.id,
          ids: ns.map((n) => n.id),
          deltaTick: dt,
          deltaPitch: dp,
        });
      }
      if (ctrl && e.key.toLowerCase() === "x") {
        e.preventDefault();
        useStudio
          .getState()
          .setClipboard(notes.filter((n) => selectedSet.has(n.id)));
        if (selected.length)
          void edit({
            type: "pianoNotes",
            section: section.id,
            channel,
            remove: notes.filter((n) => selectedSet.has(n.id)).map((n) => n.id),
          });
      }
      if (ctrl && e.key.toLowerCase() === "b") {
        e.preventDefault();
        duplicate();
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        if (selected.length) {
          e.preventDefault();
          void edit({ type: "deleteNotes", ids: selected });
        }
      }
      if (ctrl && e.key.toLowerCase() === "a") {
        e.preventDefault();
        select(notes.map((n) => n.id));
      }
      if (ctrl && e.key.toLowerCase() === "c") {
        e.preventDefault();
        useStudio
          .getState()
          .setClipboard(notes.filter((n) => selectedSet.has(n.id)));
      }
      if (ctrl && e.key.toLowerCase() === "v") {
        e.preventDefault();
        const copied = useStudio.getState().clipboard;
        if (copied.length) {
          const first = Math.min(...copied.map((n) => n.start));
          const add = copied.map((n) => ({
            ...noteInput(n),
            track: notes[0]?.track ?? 0,
            channel,
            start: section.start + cursor.current + n.start - first,
          }));
          try {
            validateDraft(add, section);
            void edit({
              type: "addNotes",
              section: section.id,
              notes: add,
            });
          } catch (e) {
            useStudio.setState({ error: String(e) });
          }
        }
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [
    notes,
    view,
    section,
    selected,
    channel,
    edit,
    select,
    busy,
    disabled,
    grid,
  ]);
  if (!view || !section) return null;
  return (
    <div className="piano">
      <PianoTools
        notes={notes}
        section={section}
        view={view}
        channel={channel}
        grid={grid}
        disabled={busy || disabled}
        row={ROW}
        onRow={(next) => {
          if (scroll.current) {
            const center =
              (scroll.current.scrollTop + size.h / 2 - RULER) / ROW;
            scroll.current.scrollTop = center * next - size.h / 2 + RULER;
          }
          setRow(next);
        }}
        noteColors={channelColors}
        onNoteColorsChange={onNoteColorsChange}
        length={length}
        onLength={setLength}
        ghosts={ghosts}
        onGhosts={setGhosts}
        follow={follow}
        onFollow={setFollow}
        scroll={scroll}
        width={size.w}
      />
      <div
        className="piano-scroll"
        ref={scroll}
        onScroll={(e) =>
          setOffset({
            x: e.currentTarget.scrollLeft,
            y: e.currentTarget.scrollTop,
          })
        }
      >
        <div
          style={{
            position: "relative",
            width: Math.max(
              size.w,
              KEY + (section.end - section.start) * scale + 80,
            ),
            height: 128 * ROW + RULER,
          }}
        >
          <canvas
            aria-label="Piano roll. Double click to add notes. Drag notes to move, drag either edge to resize. Alt bypasses snap; middle drag pans."
            role="img"
            tabIndex={0}
            ref={canvas}
            style={{
              position: "sticky",
              top: 0,
              left: 0,
              display: "block",
              touchAction: "none",
            }}
            onPointerDown={down}
            onPointerMove={move}
            onPointerUp={up}
            onPointerCancel={() => {
              anchor.current = null;
              setGesture(null);
            }}
            onLostPointerCapture={() => {
              anchor.current = null;
              setGesture(null);
            }}
            onDoubleClick={(e) => {
              if (busy || disabled) return;
              const r = e.currentTarget.getBoundingClientRect(),
                n = hit(e.clientX - r.left, e.clientY - r.top);
              if (n) {
                select([n.id]);
                document
                  .querySelector<HTMLInputElement>(
                    '.inspector input[type="number"]',
                  )
                  ?.focus();
              } else if (tool === "select")
                add(
                  e.clientX - e.currentTarget.getBoundingClientRect().left,
                  e.clientY - e.currentTarget.getBoundingClientRect().top,
                );
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              if (!e.shiftKey) return;
              const r = e.currentTarget.getBoundingClientRect(),
                n = hit(e.clientX - r.left, e.clientY - r.top);
              if (n) {
                select([n.id]);
                setMenu({ x: e.clientX, y: e.clientY, id: n.id });
              }
            }}
          />
        </div>
      </div>
      <ControllerLane
        notes={notes}
        section={section}
        view={view}
        channel={channel}
        width={size.w}
        scale={scale}
        offset={offset.x}
        grid={grid}
        disabled={busy || disabled}
        playheadTick={playheadTick}
      />
      {menu && (
        <div
          className="context-menu"
          style={{
            left: Math.min(menu.x, window.innerWidth - 180),
            top: Math.min(menu.y, window.innerHeight - 130),
          }}
          onMouseLeave={() => setMenu(null)}
        >
          <button
            onClick={() => {
              void edit({
                type: "quantize",
                section: section.id,
                ids: [menu.id],
                step: grid,
              });
              setMenu(null);
            }}
          >
            Quantize to grid
          </button>
          <button
            onClick={() => {
              void edit({ type: "deleteNotes", ids: [menu.id] });
              setMenu(null);
            }}
          >
            Delete note <kbd>Del</kbd>
          </button>
          <button
            onClick={() => {
              setNoteProps({ x: menu.x, y: menu.y, id: menu.id });
              setMenu(null);
            }}
          >
            Note properties…
          </button>
        </div>
      )}
      {noteProps &&
        (() => {
          const note = notes.find((n) => n.id === noteProps.id);
          if (!note || !section) return null;
          return (
            <div
              className="note-props-panel"
              style={{
                left: Math.min(noteProps.x, window.innerWidth - 200),
                top: Math.min(noteProps.y, window.innerHeight - 200),
              }}
              onPointerDown={(e) => e.stopPropagation()}
            >
              <header>
                <strong>{pitchName(note.pitch)}</strong>
                <button
                  type="button"
                  className="ghost"
                  aria-label="Close note properties"
                  onClick={() => setNoteProps(null)}
                >
                  ×
                </button>
              </header>
              <label>
                Start
                <input
                  type="number"
                  value={note.start - section.start}
                  min={0}
                  max={Math.max(0, section.end - section.start - 1)}
                  onChange={(e) => {
                    const n = Number(e.target.value);
                    if (!Number.isFinite(n)) return;
                    void edit({
                      type: "updateNotes",
                      section: section.id,
                      ids: [note.id],
                      deltaTick: section.start + n - note.start,
                    });
                  }}
                />
              </label>
              <label>
                Duration
                <input
                  type="number"
                  value={note.duration}
                  min={1}
                  max={Math.max(1, section.end - note.start)}
                  onChange={(e) => {
                    const n = Number(e.target.value);
                    if (!Number.isFinite(n) || n < 1) return;
                    void edit({
                      type: "updateNotes",
                      section: section.id,
                      ids: [note.id],
                      duration: n,
                    });
                  }}
                />
              </label>
              <label>
                Pitch
                <input
                  type="number"
                  value={note.pitch}
                  min={0}
                  max={127}
                  onChange={(e) => {
                    const n = Number(e.target.value);
                    if (!Number.isFinite(n)) return;
                    void edit({
                      type: "updateNotes",
                      section: section.id,
                      ids: [note.id],
                      deltaPitch: Math.max(0, Math.min(127, n)) - note.pitch,
                    });
                  }}
                />
              </label>
              <label>
                Velocity
                <input
                  type="number"
                  value={note.velocity}
                  min={1}
                  max={127}
                  onChange={(e) => {
                    const n = Number(e.target.value);
                    if (!Number.isFinite(n)) return;
                    void edit({
                      type: "updateNotes",
                      section: section.id,
                      ids: [note.id],
                      velocity: Math.max(1, Math.min(127, n)),
                    });
                  }}
                />
              </label>
              <button
                type="button"
                className="danger-link"
                onClick={() => {
                  void edit({ type: "deleteNotes", ids: [note.id] });
                  setNoteProps(null);
                }}
              >
                Delete
              </button>
            </div>
          );
        })()}
    </div>
  );
}
