import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useStudio } from "./store";
import type { Note } from "./contracts";

const KEY = 60,
  ROW = 18,
  RULER = 28;
const names = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
export const pitchName = (n: number) =>
  `${names[n % 12]}${Math.floor(n / 12) - 1}`;
interface Gesture {
  mode: "move" | "resize" | "marquee";
  x: number;
  y: number;
  dx: number;
  dy: number;
  ids: number[];
  note?: Note;
}
export function PianoRoll() {
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
  const section = view?.sections.find((s) => s.id === sectionId);
  const scroll = useRef<HTMLDivElement>(null),
    canvas = useRef<HTMLCanvasElement>(null),
    velocity = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ w: 800, h: 500 });
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [gesture, setGesture] = useState<Gesture | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: number } | null>(
    null,
  );
  const cursor = useRef(0);
  const anchor = useRef<Gesture | null>(null);
  const scale = (72 * zoom) / (view?.ppq ?? 480);
  const grid = ((view?.ppq ?? 480) * 4) / snap;
  const notes =
    view?.notes.filter(
      (n) =>
        n.channel === channel &&
        section &&
        n.start >= section.start &&
        n.start < section.end,
    ) ?? [];
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
    setGesture(null);
    anchor.current = null;
  }, [sectionId, channel]);
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
      const active = gesture?.ids.includes(n.id);
      const dt = active && gesture?.mode === "move" ? gesture.dx : 0;
      const dp = active && gesture?.mode === "move" ? gesture.dy : 0;
      const dd = active && gesture?.mode === "resize" ? gesture.dx : 0;
      return {
        x: KEY + (n.start - (section?.start ?? 0) + dt) * scale - offset.x,
        y: RULER + (127 - n.pitch - dp) * ROW - offset.y,
        w: Math.max(3, (n.duration + dd) * scale),
        h: ROW - 2,
      };
    },
    [gesture, section?.start, scale, offset],
  );
  useEffect(() => {
    if (!canvas.current || !velocity.current || !view || !section) return;
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
      const black = [1, 3, 6, 8, 10].includes(p % 12);
      ctx.fillStyle = black ? "#171a1d" : "#1d2023";
      ctx.fillRect(KEY, y, size.w - KEY, ROW);
      ctx.fillStyle = black ? "#26292d" : "#bbc0c1";
      ctx.fillRect(0, y, KEY - 1, ROW - 1);
      ctx.fillStyle = black ? "#91999e" : "#3f4549";
      if (p % 12 === 0 || p % 12 === 5) ctx.fillText(pitchName(p), 22, y + 12);
      if (p % 12 === 0) {
        ctx.strokeStyle = "#353a3e";
        ctx.beginPath();
        ctx.moveTo(KEY, y + ROW);
        ctx.lineTo(size.w, y + ROW);
        ctx.stroke();
      }
    }
    const beat = (view.ppq * 4) / view.denominator,
      bar = beat * view.numerator;
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
    for (const n of notes) {
      const r = rect(n);
      if (r.x + r.w < KEY || r.x > size.w || r.y + r.h < RULER || r.y > size.h)
        continue;
      ctx.save();
      ctx.beginPath();
      ctx.rect(KEY, RULER, size.w - KEY, size.h - RULER);
      ctx.clip();
      const chosen = selected.includes(n.id);
      ctx.fillStyle = chosen
        ? "#f0cf91"
        : `hsl(39 52% ${38 + (n.velocity / 127) * 28}%)`;
      ctx.beginPath();
      ctx.roundRect(r.x, r.y + 1, r.w, r.h, 2);
      ctx.fill();
      if (chosen) {
        ctx.strokeStyle = "#fff0c8";
        ctx.stroke();
      }
      if (r.w > 40) {
        ctx.fillStyle = "#29251d";
        ctx.fillText(pitchName(n.pitch), r.x + 5, r.y + 12);
      }
      ctx.restore();
    }
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
    const vc = velocity.current;
    vc.width = size.w * dpr;
    vc.height = 80 * dpr;
    vc.style.width = `${size.w}px`;
    vc.style.height = "80px";
    const v = vc.getContext("2d");
    if (!v) return;
    v.scale(dpr, dpr);
    v.fillStyle = "#171a1c";
    v.fillRect(0, 0, size.w, 80);
    v.fillStyle = "#a0a8ad";
    v.font = "10px Segoe UI";
    v.fillText("VELOCITY", 9, 16);
    for (const n of notes) {
      const x = KEY + (n.start - section.start) * scale - offset.x;
      if (x < KEY || x > size.w) continue;
      v.fillStyle = selected.includes(n.id) ? "#f0cf91" : "#9c845b";
      v.fillRect(
        x,
        77 - (n.velocity / 127) * 55,
        Math.max(2, Math.min(8, n.duration * scale)),
        (n.velocity / 127) * 55,
      );
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
  const tickAt = (x: number) =>
    Math.max(0, Math.round((x - KEY + offset.x) / scale / grid) * grid);
  const add = (x: number, y: number) => {
    if (!section || !view || busy || x < KEY || y < RULER) return;
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
          duration: Math.min(grid, section.end - section.start - local),
        },
      ],
    });
  };
  const down = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0 || busy) return;
    setMenu(null);
    const { x, y } = point(e);
    const n = hit(x, y);
    if (!section || x < KEY || y < RULER) return;
    cursor.current = tickAt(x);
    if (!n && tool === "draw") {
      add(x, y);
      return;
    }
    if (n) {
      const ids = e.shiftKey
        ? selected.includes(n.id)
          ? selected.filter((id) => id !== n.id)
          : [...selected, n.id]
        : selected.includes(n.id)
          ? selected
          : [n.id];
      select(ids);
      const r = rect(n);
      anchor.current = {
        mode: x > r.x + r.w - 7 ? "resize" : "move",
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
    if (!a) return;
    const { x, y } = point(e);
    const dx =
      a.mode === "marquee"
        ? x - a.x
        : Math.round((x - a.x) / scale / grid) * grid;
    const dy = a.mode === "marquee" ? y - a.y : Math.round((a.y - y) / ROW);
    const g = { ...a, dx, dy };
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
    const g = gesture;
    anchor.current = null;
    setGesture(null);
    if (!g || !section || g.mode === "marquee" || !g.note) return;
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
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (document.querySelector('[aria-modal="true"]')) return;
      if (
        (e.target as HTMLElement).closest(
          'input,select,textarea,[contenteditable="true"]',
        ) ||
        !section ||
        !view
      )
        return;
      const ctrl = e.ctrlKey || e.metaKey;
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
        useStudio.setState({
          clipboard: notes.filter((n) => selected.includes(n.id)),
        });
      }
      if (ctrl && e.key.toLowerCase() === "v") {
        e.preventDefault();
        const copied = useStudio.getState().clipboard;
        if (copied.length) {
          const first = Math.min(...copied.map((n) => n.start));
          void edit({
            type: "addNotes",
            section: section.id,
            notes: copied.map((n) => ({
              track: n.track,
              channel,
              pitch: n.pitch,
              velocity: n.velocity,
              start: section.start + cursor.current + n.start - first,
              duration: n.duration,
            })),
          });
        }
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [notes, view, section, selected, channel, edit, select]);
  if (!view || !section) return null;
  return (
    <div className="piano">
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
            aria-label="Piano roll. Double click to add notes. Drag notes to move, drag the right edge to resize."
            role="img"
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
            onDoubleClick={(e) => {
              if (tool !== "draw")
                add(
                  e.clientX - e.currentTarget.getBoundingClientRect().left,
                  e.clientY - e.currentTarget.getBoundingClientRect().top,
                );
            }}
            onContextMenu={(e) => {
              e.preventDefault();
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
      <canvas
        className="velocity"
        ref={velocity}
        aria-label="Velocity lane"
        onPointerDown={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const x = e.clientX - r.left;
          const n = notes.reduce<Note | undefined>((best, n) => {
            const nx = KEY + (n.start - section.start) * scale - offset.x;
            return Math.abs(nx - x) < 10 &&
              (!best ||
                Math.abs(nx - x) <
                  Math.abs(
                    KEY + (best.start - section.start) * scale - offset.x - x,
                  ))
              ? n
              : best;
          }, undefined);
          if (n)
            void edit({
              type: "updateNotes",
              section: section.id,
              ids: [n.id],
              velocity: Math.max(
                1,
                Math.min(
                  127,
                  Math.round(((77 - (e.clientY - r.top)) / 55) * 127),
                ),
              ),
            });
        }}
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
        </div>
      )}
    </div>
  );
}
