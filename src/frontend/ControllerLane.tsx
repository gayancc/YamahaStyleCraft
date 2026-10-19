import { useEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent } from "react";
import type { Note, Section, View } from "./contracts";
import { useStudio } from "./store";
import {
  bend14,
  clamp,
  controllerLanes,
  EQ_HIGH_LANE,
  EQ_LOW_LANE,
  eqBandFromLane,
  eqLanePoints,
  eqValueBefore,
  meterAt,
  noteUpdate,
  PITCH_BEND_LANE,
  TEMPO_LANE,
  tempoBpmFromMeta,
  tickSnap,
} from "./pianoEditing";
import { curveT, isSpanCurve } from "./controllerCurves";

const KEY = 60;
interface Draft {
  x: number;
  y: number;
  lastX: number;
  lastY: number;
  revision: number;
  values: Map<number, number>;
  points: Map<number, number>;
  remove: Set<number>;
  ids: number[];
  originals: Map<number, number>;
  erase: boolean;
}
export function ControllerLane({
  notes,
  section,
  view,
  channel,
  width,
  scale,
  offset,
  grid,
  disabled,
  playheadTick,
}: {
  notes: Note[];
  section: Section;
  view: View;
  channel: number;
  width: number;
  scale: number;
  offset: number;
  grid: number;
  disabled: boolean;
  playheadTick: number | null;
}) {
  const { selected, edit } = useStudio();
  const [lane, setLane] = useState(-1),
    [mode, setMode] = useState("draw"),
    [height, setHeight] = useState(112);
  const [value, setValue] = useState(90),
    [ids, setIds] = useState<number[]>([]),
    [draft, setDraft] = useState<Draft | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null),
    anchor = useRef<Draft | null>(null);
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const chosen = notes.filter((n) => selectedSet.has(n.id));
  const eqBand = eqBandFromLane(lane);
  const isBend = lane === PITCH_BEND_LANE;
  const isTempo = lane === TEMPO_LANE;
  const laneMax = lane === -1 ? 127 : isBend ? 16383 : isTempo ? 400 : 127;
  const laneMin = lane === -1 ? 1 : isTempo ? 20 : 0;
  const readValue = (data: number[]) =>
    isBend
      ? bend14(data)
      : isTempo
        ? Math.round(tempoBpmFromMeta(data) ?? view.tempo)
        : (data[2] ?? 0);
  const events = useMemo(() => {
    if (eqBand)
      return eqLanePoints(
        view.events,
        section.start,
        section.end,
        channel,
        eqBand,
      );
    if (isTempo)
      return view.events
        .filter(
          (e) =>
            e.tick >= section.start &&
            e.tick < section.end &&
            tempoBpmFromMeta(e.data) != null,
        )
        .sort((a, b) => a.tick - b.tick || a.order - b.order);
    if (isBend)
      return view.events
        .filter(
          (e) =>
            e.tick >= section.start &&
            e.tick < section.end &&
            e.data.length === 3 &&
            e.data[0] === (0xe0 | (channel - 1)),
        )
        .sort((a, b) => a.tick - b.tick || a.order - b.order);
    return view.events
      .filter(
        (e) =>
          e.tick >= section.start &&
          e.tick < section.end &&
          e.data.length === 3 &&
          e.data[0] === (0xb0 | (channel - 1)) &&
          e.data[1] === lane,
      )
      .sort((a, b) => a.tick - b.tick || a.order - b.order);
  }, [view, section, channel, lane, eqBand, isBend, isTempo]);
  const cancel = () => {
    anchor.current = null;
    setDraft(null);
  };
  useEffect(() => {
    cancel();
    setIds([]);
  }, [section.id, channel, lane, view]);
  useEffect(() => {
    if (disabled) cancel();
  }, [disabled]);
  const px = (tick: number) => KEY + (tick - section.start) * scale - offset;
  // Velocity keeps a 0..127 draw domain (clamped to 1..127); tempo uses 20..400.
  const mapMin = lane === -1 ? 0 : laneMin;
  const laneSpan = Math.max(1, laneMax - mapMin);
  const py = (v: number) =>
    height - 8 - ((v - mapMin) / laneSpan) * (height - 24);
  const valueAt = (y: number) =>
    clamp(
      Math.round(mapMin + ((height - 8 - y) / (height - 24)) * laneSpan),
      laneMin,
      laneMax,
    );
  const tickAt = (x: number) =>
    section.start +
    clamp(
      tickSnap((x - KEY + offset) / scale, grid),
      0,
      section.end - section.start - 1,
    );
  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(width * dpr);
    c.height = Math.round(height * dpr);
    c.style.width = `${width}px`;
    c.style.height = `${height}px`;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.fillStyle = "#26363e";
    ctx.fillRect(0, 0, width, height);
    ctx.font = "10px Segoe UI";
    ctx.save();
    ctx.beginPath();
    ctx.rect(KEY, 0, width - KEY, height);
    ctx.clip();
    for (const v of isTempo ? [60, 120, 180, 240] : [0, 64, 127]) {
      ctx.fillStyle = "#53636a60";
      ctx.fillRect(KEY, py(v), width - KEY, 1);
    }
    const stride = Math.max(1, Math.ceil(6 / (grid * scale)));
    for (
      let i = Math.max(0, Math.floor(offset / (grid * scale)));
      i * grid < section.end - section.start &&
      KEY + i * grid * scale - offset < width;
      i += stride
    ) {
      ctx.fillStyle = i % 4 === 0 ? "#172a34" : "#2b3b43";
      ctx.fillRect(KEY + i * grid * scale - offset, 0, 1, height);
    }
    const { beat, bar } = meterAt(view, section.start);
    for (
      let i = Math.max(0, Math.floor(offset / (beat * scale)));
      i * beat < section.end - section.start &&
      KEY + i * beat * scale - offset < width;
      i++
    ) {
      ctx.fillStyle = i % Math.round(bar / beat) === 0 ? "#52616a" : "#3b4b54";
      ctx.fillRect(KEY + i * beat * scale - offset, 0, 1, height);
    }
    if (lane === -1) {
      for (const n of notes) {
        const x = px(n.start);
        if (x < KEY - 4 || x > width + 4) continue;
        const y = py(draft?.values.get(n.id) ?? n.velocity);
        ctx.fillStyle = selectedSet.has(n.id)
          ? "#e5a2ab"
          : selected.length
            ? "#8bb69480"
            : "#9ac89d";
        ctx.fillRect(x, height - 8, 2, y - height + 8);
        ctx.beginPath();
        ctx.arc(x + 1, y, 3, 0, Math.PI * 2);
        ctx.fill();
        if (selectedSet.has(n.id))
          ctx.fillRect(x, y, Math.max(3, Math.min(80, n.duration * scale)), 1);
      }
    } else {
      const data = new Map(
        events
          .filter((e) => !draft?.remove.has(e.id))
          .map((e) => [e.tick, { value: readValue(e.data), id: e.id }]),
      );
      for (const [tick, v] of draft?.points ?? [])
        data.set(tick, { value: v, id: data.get(tick)?.id ?? -1 });
      let lastX = px(section.start);
      let lastY: number | null = null;
      if (eqBand) {
        const prior = eqValueBefore(
          view.events,
          section.start,
          channel,
          eqBand,
        );
        lastY = prior != null ? py(prior) : null;
      } else if (isTempo) {
        const baseline = view.events
          .filter(
            (e) => e.tick < section.start && tempoBpmFromMeta(e.data) != null,
          )
          .sort((a, b) => a.tick - b.tick || a.order - b.order)
          .at(-1);
        lastY = baseline
          ? py(readValue(baseline.data))
          : py(Math.round(view.tempo));
      } else {
        const baseline = view.events
          .filter(
            (e) =>
              e.tick < section.start &&
              e.data.length === 3 &&
              (isBend
                ? e.data[0] === (0xe0 | (channel - 1))
                : e.data[0] === (0xb0 | (channel - 1)) && e.data[1] === lane),
          )
          .sort((a, b) => a.tick - b.tick || a.order - b.order)
          .at(-1);
        lastY = baseline ? py(readValue(baseline.data)) : null;
      }
      ctx.beginPath();
      ctx.strokeStyle = "#8cbfcd";
      ctx.lineWidth = 1.5;
      for (const [tick, p] of [...data].sort((a, b) => a[0] - b[0])) {
        const x = px(tick),
          y = py(p.value);
        if (lastY !== null) {
          ctx.moveTo(lastX, lastY);
          ctx.lineTo(x, lastY);
          ctx.lineTo(x, y);
        }
        lastX = x;
        lastY = y;
      }
      if (lastY !== null) {
        ctx.moveTo(lastX, lastY);
        ctx.lineTo(Math.min(width, px(section.end)), lastY);
      }
      ctx.stroke();
      for (const [tick, p] of data) {
        const x = px(tick);
        if (x < KEY || x > width) continue;
        ctx.fillStyle = ids.includes(p.id) ? "#e5a2ab" : "#a3d3dd";
        ctx.beginPath();
        ctx.arc(x, py(p.value), 3.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    if (playheadTick !== null) {
      ctx.fillStyle = "#e8c277";
      ctx.fillRect(px(playheadTick), 0, 1.5, height);
    }
    // Live SFM help: “drag the yellow handle … to shape the curve”.
    if (draft && isSpanCurve(mode) && Math.hypot(draft.lastX - draft.x, draft.lastY - draft.y) > 4) {
      const midX = (draft.x + draft.lastX) / 2;
      const midY =
        draft.y +
        (draft.lastY - draft.y) * curveT(mode, 0.5);
      ctx.fillStyle = "#e8c277";
      ctx.strokeStyle = "#1a2228";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(midX, midY, 5, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }
    ctx.restore();
    ctx.fillStyle = "#344149";
    ctx.fillRect(0, 0, KEY, height);
    ctx.fillStyle = "#aab8bd";
    ctx.fillText(
      lane === -1
        ? "VEL"
        : eqBand
          ? eqBand === "eqLow"
            ? "EQ Lo"
            : "EQ Hi"
          : isBend
            ? "BEND"
            : isTempo
              ? "TEMPO"
              : `CC ${lane}`,
      10,
      16,
    );
    ctx.fillText(String(laneMax), 8, 34);
    ctx.fillText(String(laneMin), 24, height - 8);
  });
  const point = (e: PointerEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  /** Returns null when the curve would exceed the density guard. */
  const sample = (
    a: Draft,
    x: number,
    y: number,
    first = false,
  ): Draft | null => {
    const g = {
      ...a,
      lastX: x,
      lastY: y,
      values: new Map(a.values),
      points: new Map(a.points),
      remove: new Set(a.remove),
    };
    const span = isSpanCurve(mode);
    const from = span ? a.x : a.lastX,
      fromY = span ? a.y : a.lastY,
      left = Math.min(from, x),
      right = Math.max(from, x);
    const at = (p: number) =>
      valueAt(
        Math.abs(x - from) < 1
          ? y
          : fromY +
              (y - fromY) *
                curveT(mode, clamp((p - from) / (x - from), 0, 1)),
      );
    if (lane === -1) {
      const eligible = chosen.length ? chosen : notes;
      if (first) {
        const near = eligible
          .filter((n) => Math.abs(px(n.start) - x) <= 9)
          .sort(
            (n, m) =>
              Math.abs(py(n.velocity) - y) - Math.abs(py(m.velocity) - y),
          )[0];
        if (near) {
          g.ids = chosen.length ? chosen.map((n) => n.id) : [near.id];
          g.originals = new Map(
            eligible
              .filter((n) => g.ids.includes(n.id))
              .map((n) => [n.id, n.velocity]),
          );
          if (mode !== "relative") g.values.set(near.id, valueAt(y));
        }
      } else if (mode === "relative") {
        const delta = Math.round(((a.y - y) / (height - 24)) * 127);
        for (const [id, v] of a.originals)
          g.values.set(id, clamp(v + delta, 1, 127));
      } else if (Math.abs(x - a.x) < 4) {
        for (const id of a.ids) {
          const n = notes.find((n) => n.id === id);
          if (n && Math.abs(px(n.start) - x) <= 9) g.values.set(id, valueAt(y));
        }
      } else {
        if (span) g.values.clear();
        for (const n of eligible)
          if (px(n.start) >= left - 2 && px(n.start) <= right + 2)
            g.values.set(n.id, at(px(n.start)));
      }
    } else if (a.erase) {
      for (const e of events)
        if (px(e.tick) >= left - 7 && px(e.tick) <= right + 7)
          g.remove.add(e.id);
    } else if (mode === "relative") {
      const delta = Math.round(((a.y - y) / (height - 24)) * laneSpan);
      for (const e of events)
        if (a.ids.includes(e.id))
          g.points.set(
            e.tick,
            clamp(readValue(e.data) + delta, laneMin, laneMax),
          );
    } else {
      if (span) g.points.clear();
      const lo = Math.min(tickAt(from), tickAt(x)),
        hi = Math.max(tickAt(from), tickAt(x)),
        count = Math.ceil((hi - lo) / Math.max(1, grid)) + 1;
      if (count > 10000) return null;
      const ticks = new Set([lo, hi]);
      for (let i = 0; i < count; i++)
        ticks.add(Math.min(hi, Math.round(lo + i * Math.max(1, grid))));
      for (const e of events)
        if (e.tick >= lo && e.tick <= hi) ticks.add(e.tick);
      for (const tick of ticks) g.points.set(tick, at(px(tick)));
    }
    return g;
  };
  const reportDense = () =>
    useStudio.setState({
      error:
        "Controller curve is too dense. Use a coarser grid or shorter span.",
    });
  const down = (e: PointerEvent<HTMLCanvasElement>) => {
    if (
      disabled ||
      ![0, 2].includes(e.button) ||
      (lane === -1 && e.button === 2)
    )
      return;
    const { x, y } = point(e);
    if (x < KEY || (x - KEY + offset) / scale >= section.end - section.start)
      return;
    e.currentTarget.focus();
    let selection = ids;
    if (lane !== -1 && mode === "relative") {
      const nearest = events
        .filter((e) => Math.abs(px(e.tick) - x) < 9)
        .sort(
          (a, b) =>
            Math.abs(py(readValue(a.data)) - y) -
            Math.abs(py(readValue(b.data)) - y),
        )[0];
      selection = nearest
        ? e.shiftKey
          ? ids.includes(nearest.id)
            ? ids.filter((id) => id !== nearest.id)
            : [...ids, nearest.id]
          : ids.includes(nearest.id)
            ? ids
            : [nearest.id]
        : [];
      setIds(selection);
    }
    const a: Draft = {
      x,
      y,
      lastX: x,
      lastY: y,
      revision: view.revision,
      values: new Map(),
      points: new Map(),
      remove: new Set(),
      ids: selection,
      originals: new Map(),
      erase: e.button === 2,
    };
    const g = sample(a, x, y, true);
    if (!g) {
      reportDense();
      return;
    }
    anchor.current = g;
    setDraft(g);
    setValue(valueAt(y));
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const move = (e: PointerEvent<HTMLCanvasElement>) => {
    const a = anchor.current;
    if (!a || disabled) return;
    const { x, y } = point(e);
    const g = sample(a, x, y);
    if (!g) {
      reportDense();
      return;
    }
    anchor.current = g;
    setDraft(g);
    setValue(valueAt(y));
  };
  const up = () => {
    const g = anchor.current;
    cancel();
    if (!g || disabled || g.revision !== view.revision) return;
    if (lane === -1) {
      const updates = notes
        .filter((n) => g.values.has(n.id))
        .map((n) => ({ ...noteUpdate(n), velocity: g.values.get(n.id)! }));
      if (updates.length)
        void edit({
          type: "pianoNotes",
          section: section.id,
          channel,
          updates,
        });
    } else if (g.points.size || g.remove.size) {
      const pts = [...g.points]
        .sort((a, b) => a[0] - b[0])
        .map(([tick, value]) => ({ tick, value }));
      if (eqBand) {
        const clearTicks = [
          ...new Set(
            events.filter((e) => g.remove.has(e.id)).map((e) => e.tick),
          ),
        ];
        void edit({
          type: "mixerEqPoints",
          section: section.id,
          channel,
          band: eqBand,
          points: pts,
          clearTicks,
        });
      } else if (isBend)
        void edit({
          type: "pitchBendPoints",
          section: section.id,
          channel,
          points: pts,
          remove: [...g.remove],
        });
      else if (isTempo)
        void edit({
          type: "tempoPoints",
          section: section.id,
          points: pts.map(({ tick, value }) => ({ tick, bpm: value })),
          remove: [...g.remove],
        });
      else
        void edit({
          type: "controllerPoints",
          section: section.id,
          channel,
          controller: lane,
          points: pts,
          remove: [...g.remove],
        });
    }
  };
  const setSelected = () => {
    if (disabled) return;
    if (lane === -1 && chosen.length)
      void edit({
        type: "pianoNotes",
        section: section.id,
        channel,
        updates: chosen.map((n) => ({
          ...noteUpdate(n),
          velocity: Math.max(1, value),
        })),
      });
    else if (lane !== -1 && ids.length) {
      const ticks = [
        ...new Set(
          events.filter((e) => ids.includes(e.id)).map((e) => e.tick),
        ),
      ].map((tick) => ({ tick, value }));
      if (eqBand)
        void edit({
          type: "mixerEqPoints",
          section: section.id,
          channel,
          band: eqBand,
          points: ticks,
        });
      else if (isBend)
        void edit({
          type: "pitchBendPoints",
          section: section.id,
          channel,
          points: ticks,
        });
      else if (isTempo)
        void edit({
          type: "tempoPoints",
          section: section.id,
          points: ticks.map(({ tick, value }) => ({ tick, bpm: value })),
        });
      else
        void edit({
          type: "controllerPoints",
          section: section.id,
          channel,
          controller: lane,
          points: ticks,
        });
    }
  };
  return (
    <div
      className="controller-lane"
      onKeyDown={(e) => {
        if ((e.target as HTMLElement).matches("input,select") || disabled)
          return;
        if (e.key === "Escape") {
          cancel();
          e.stopPropagation();
        }
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
          e.preventDefault();
          e.stopPropagation();
          if (lane === -1) useStudio.getState().select(notes.map((n) => n.id));
          else setIds(events.map((e) => e.id));
        }
        if ((e.key === "Delete" || e.key === "Backspace") && lane !== -1) {
          e.preventDefault();
          e.stopPropagation();
          if (ids.length) {
            if (eqBand)
              void edit({
                type: "mixerEqPoints",
                section: section.id,
                channel,
                band: eqBand,
                clearTicks: [
                  ...new Set(
                    events
                      .filter((ev) => ids.includes(ev.id))
                      .map((ev) => ev.tick),
                  ),
                ],
              });
            else if (isBend)
              void edit({
                type: "pitchBendPoints",
                section: section.id,
                channel,
                remove: ids,
              });
            else if (isTempo)
              void edit({
                type: "tempoPoints",
                section: section.id,
                remove: ids,
              });
            else
              void edit({
                type: "controllerPoints",
                section: section.id,
                channel,
                controller: lane,
                remove: ids,
              });
          }
        }
      }}
    >
      <div
        className="lane-splitter"
        role="separator"
        aria-label="Controller lane height"
        aria-orientation="horizontal"
        aria-valuenow={height}
        aria-valuemin={64}
        aria-valuemax={240}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            e.preventDefault();
            e.stopPropagation();
            setHeight((h) =>
              clamp(h + (e.key === "ArrowUp" ? 8 : -8), 64, 240),
            );
          }
        }}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          e.currentTarget.dataset.y = String(e.clientY);
          e.currentTarget.dataset.h = String(height);
        }}
        onPointerMove={(e) => {
          if (e.currentTarget.hasPointerCapture(e.pointerId))
            setHeight(
              clamp(
                +(e.currentTarget.dataset.h ?? 112) +
                  +(e.currentTarget.dataset.y ?? e.clientY) -
                  e.clientY,
                64,
                240,
              ),
            );
        }}
        onPointerUp={(e) => e.currentTarget.releasePointerCapture(e.pointerId)}
        onDoubleClick={() => setHeight(112)}
      />
      <div className="lane-toolbar">
          <select
          aria-label="Controller lane"
          value={lane}
          onChange={(e) => {
            const next = +e.target.value;
            setLane(next);
            if (next === TEMPO_LANE)
              setValue(clamp(Math.round(view.tempo), 20, 400));
            else if (next === PITCH_BEND_LANE) setValue(8192);
            else if (next === -1) setValue(90);
            else setValue(64);
          }}
        >
          <option value={-1}>Note velocity</option>
          <option value={EQ_LOW_LANE}>EQ Low · NRPN</option>
          <option value={EQ_HIGH_LANE}>EQ High · NRPN</option>
          <option value={PITCH_BEND_LANE}>Pitch bend</option>
          <option value={TEMPO_LANE}>Tempo (BPM)</option>
          {controllerLanes.map(([id, name]) => (
            <option key={id} value={id}>
              {name}
            </option>
          ))}
        </select>
        <select
          aria-label="Controller gesture"
          value={mode}
          onChange={(e) => {
            cancel();
            setMode(e.target.value);
          }}
        >
          <option value="draw">Draw</option>
          <option value="ramp">Linear</option>
          <option value="easeIn">Ease In</option>
          <option value="easeOut">Ease Out</option>
          <option value="step">Step</option>
          <option value="relative">Select / relative</option>
        </select>
        <input
          aria-label="Lane selection value"
          type="number"
          min={laneMin}
          max={laneMax}
          value={value}
          onChange={(e) =>
            setValue(clamp(+e.target.value, laneMin, laneMax))
          }
        />
        <button
          disabled={disabled || !(lane === -1 ? chosen.length : ids.length)}
          onClick={setSelected}
        >
          Set selected
        </button>
        <span>
          {lane === -1
            ? "Note-on · 1–127"
            : eqBand
              ? `EQ NRPN · ch ${channel} · 0–127 (64=flat)`
              : isBend
                ? `Pitch bend · ch ${channel} · 0–16383 (8192=center)`
                : isTempo
                  ? "Section tempo · BPM 20–400 (SMF meta FF 51)"
                  : `Source ch ${channel} · 0–127`}
        </span>
      </div>
      <canvas
        ref={canvas}
        className="velocity"
        tabIndex={0}
        aria-label={
          lane === -1
            ? "Velocity lane. For Linear/Ease/Step, drag between points; yellow handle shows curve shape."
            : eqBand
              ? `EQ ${eqBand === "eqLow" ? "Low" : "High"} NRPN lane. Draw timed EQ automation as Genos NRPN packets.`
              : isBend
                ? "Pitch bend lane. Draw 14-bit bend automation (0–16383, center 8192)."
                : isTempo
                  ? "Tempo lane. Draw section BPM automation as SMF tempo meta (20–400)."
                  : `MIDI CC${lane} lane. Click start and end points, then drag to shape the curve (yellow handle).`
        }
        title="Click start and end points, then drag to shape the curve (yellow handle for Linear/Ease/Step)."
        style={{ touchAction: "none" }}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={cancel}
        onLostPointerCapture={cancel}
        onContextMenu={(e) => e.preventDefault()}
      />
    </div>
  );
}
