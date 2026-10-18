import { useEffect, useRef, useState } from "react";
import type {
  Region,
  Route,
  RouteChange,
  RuleField,
  Section,
  SourceAction,
} from "./contracts";
import { useStudio } from "./store";
import { partDefaultBytes } from "./casmDefaults";

const chords = [
  "Major",
  "6",
  "Maj7",
  "Maj7(#11)",
  "add9",
  "Maj7(9)",
  "6(9)",
  "aug",
  "Minor",
  "m6",
  "m7",
  "m7b5",
  "m(add9)",
  "m7(9)",
  "m7(11)",
  "mMaj7",
  "mMaj7(9)",
  "dim",
  "dim7",
  "7",
  "7sus4",
  "7b5",
  "7(9)",
  "7(#11)",
  "7(13)",
  "7(b9)",
  "7(b13)",
  "7(#9)",
  "Maj7aug",
  "7aug",
  "1+8",
  "1+5",
  "sus4",
  "sus2",
];
const roots = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];
const tables = [
  "Bypass",
  "Melody",
  "Chord",
  "Melodic Minor",
  "Melodic Minor 5th",
  "Harmonic Minor",
  "Harmonic Minor 5th",
  "Natural Minor",
  "Natural Minor 5th",
  "Dorian",
  "Dorian 5th",
];
const regionOffsets: Record<Region, number> = { low: 22, middle: 28, high: 34 };
const enabled = (b: number[], id: number) =>
  Boolean(b[17 - Math.floor(id / 8)] & (1 << (id % 8)));

function Choice({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: number;
  options: string[];
  onChange: (value: number) => void;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <select
        aria-label={label}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      >
        {options.map((name, id) => (
          <option key={id} value={id}>
            {name}
          </option>
        ))}
        {value >= options.length && (
          <option value={value}>Raw {value} · preserved</option>
        )}
      </select>
    </label>
  );
}
function Numeric({
  label,
  value,
  max = 127,
  onChange,
}: {
  label: string;
  value: number;
  max?: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <input
        aria-label={label}
        type="number"
        min={0}
        max={max}
        value={value}
        onChange={(e) => {
          if (
            e.target.value !== "" &&
            Number.isSafeInteger(e.target.valueAsNumber)
          )
            onChange(e.target.valueAsNumber);
        }}
      />
    </label>
  );
}

export function SourceTracks({
  route,
  section,
  onClose,
}: {
  route: Route;
  section: Section;
  onClose: () => void;
}) {
  const { view, busy, edit } = useStudio();
  const [revision] = useState(view!.revision);
  const [bytes, setBytes] = useState(() => [...route.bytes]);
  const [resetDefaults, setResetDefaults] = useState(false);
  const [mask, setMask] = useState(() =>
    chords.map((_, id) => enabled(route.bytes, id)),
  );
  const [exclusive, setExclusive] = useState(false);
  const [target, setTarget] = useState(0);
  const [region, setRegion] = useState<Region>("middle");
  const [error, setError] = useState<string | null>(null);
  const dialog = useRef<HTMLElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    return () => previous?.focus();
  }, []);
  const used = new Set([
    ...view!.routes
      .filter((r) => r.sections.includes(section.label))
      .map((r) => r.source),
    ...view!.events
      .filter(
        (e) =>
          e.tick >= section.start && e.tick < section.end && e.data[0] < 0xf0,
      )
      .map((e) => (e.data[0] & 15) + 1),
    ...view!.notes
      .filter(
        (n) => n.start < section.end && n.start + n.duration > section.start,
      )
      .map((n) => n.channel),
  ]);
  const o = regionOffsets[region];
  const rule = (field: RuleField, value: number) =>
    update({ type: "rule", region, field, value });
  const update = (change: RouteChange) => {
    const next = [...bytes];
    if (change.type === "root") next[18] = change.value;
    else if (change.type === "rootEnabled") {
      const index = 12 - Math.floor(change.root / 8),
        bit = 1 << (change.root % 8);
      next[index] = change.enabled ? next[index] | bit : next[index] & ~bit;
    } else if (change.type === "autoStart")
      next[13] = change.enabled ? next[13] | 4 : next[13] & ~4;
    else if (change.type === "chord") next[19] = change.value;
    else if (change.type === "zone") {
      next[20] = change.low;
      next[21] = change.high;
    } else if (change.type === "rule") {
      const offset = regionOffsets[change.region],
        value = change.value;
      if (change.field === "ntr") {
        if ((next[offset] === 2) !== (value === 2)) next[offset + 1] &= 128;
        next[offset] = value;
      } else if (change.field === "ntt")
        next[offset + 1] = (next[offset + 1] & 128) | value;
      else if (change.field === "bass")
        next[offset + 1] = (next[offset + 1] & 127) | (value << 7);
      else
        next[
          offset +
            { highKey: 2, noteLow: 3, noteHigh: 4, retrigger: 5 }[change.field]
        ] = value;
    }
    setBytes(next);
    setError(null);
  };
  // Submit final values, rather than invalid intermediate number-input states.
  const original = resetDefaults ? partDefaultBytes(route.bytes) : route.bytes;
  const changes: RouteChange[] = resetDefaults
    ? [{ type: "partDefaults" }]
    : [];
  for (let root = 0; root < 12; root++) {
    const index = 12 - Math.floor(root / 8),
      bit = 1 << (root % 8);
    if (Boolean(bytes[index] & bit) !== Boolean(original[index] & bit))
      changes.push({
        type: "rootEnabled",
        root,
        enabled: Boolean(bytes[index] & bit),
      });
  }
  if (Boolean(bytes[13] & 4) !== Boolean(original[13] & 4))
    changes.push({ type: "autoStart", enabled: Boolean(bytes[13] & 4) });
  if (bytes[18] !== original[18])
    changes.push({ type: "root", value: bytes[18] });
  if (bytes[19] !== original[19])
    changes.push({ type: "chord", value: bytes[19] });
  if (bytes[20] !== original[20] || bytes[21] !== original[21])
    changes.push({ type: "zone", low: bytes[20], high: bytes[21] });
  for (const name of ["low", "middle", "high"] as Region[]) {
    const offset = regionOffsets[name];
    const pushRule = (field: RuleField, value: number) =>
      changes.push({ type: "rule", region: name, field, value });
    if (bytes[offset] !== original[offset]) pushRule("ntr", bytes[offset]);
    const expectedTable =
      (bytes[offset] === 2) !== (original[offset] === 2)
        ? 0
        : original[offset + 1] & 127;
    if ((bytes[offset + 1] & 127) !== expectedTable)
      pushRule("ntt", bytes[offset + 1] & 127);
    if ((bytes[offset + 1] & 128) !== (original[offset + 1] & 128))
      pushRule("bass", bytes[offset + 1] >> 7);
    if (bytes[offset + 2] !== original[offset + 2])
      pushRule("highKey", bytes[offset + 2]);
    if (
      bytes[offset + 3] !== original[offset + 3] ||
      bytes[offset + 4] !== original[offset + 4]
    )
      changes.push({
        type: "limits",
        region: name,
        low: bytes[offset + 3],
        high: bytes[offset + 4],
      });
    if (bytes[offset + 5] !== original[offset + 5])
      pushRule("retrigger", bytes[offset + 5]);
  }
  const maskChanged = mask.some((value, id) => value !== enabled(original, id));
  const invalid =
    bytes[20] < 0 ||
    bytes[21] > 127 ||
    bytes[20] > bytes[21] ||
    [22, 28, 34].some(
      (offset) =>
        bytes[offset + 3] < 0 ||
        bytes[offset + 4] > 127 ||
        bytes[offset + 3] > bytes[offset + 4],
    );
  const apply = async () => {
    if (useStudio.getState().view?.revision !== revision) {
      setError("The document changed. Cancel and reopen this draft.");
      return;
    }
    const actions: SourceAction[] = [];
    if (changes.length || maskChanged || exclusive)
      actions.push({
        type: "edit",
        source: route.source,
        destination: route.destination,
        changes,
        chords:
          maskChanged || exclusive
            ? mask.flatMap((on, id) => (on ? [id] : []))
            : null,
        exclusive,
      });
    if (target)
      actions.push({
        type: "clone",
        source: route.source,
        destination: route.destination,
        target,
      });
    await edit({ type: "sourceTracks", section: section.id, actions });
    const state = useStudio.getState();
    if (state.error) setError(state.error);
    else {
      if (target) state.setChannel(target);
      onClose();
    }
  };
  return (
    <div
      className="modal-backdrop"
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" && !busy) {
          e.preventDefault();
          close.current();
        }
        if (e.key === "Tab") {
          const controls = Array.from(
            dialog.current?.querySelectorAll<HTMLElement>(
              "button:not(:disabled),input:not(:disabled),select:not(:disabled)",
            ) ?? [],
          );
          const first = controls[0],
            last = controls.at(-1);
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last?.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first?.focus();
          }
        }
      }}
    >
      <section
        ref={dialog}
        className="modal source-tracks-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="source-tracks-title"
      >
        <div className="modal-title">
          <h2 id="source-tracks-title">Source tracks</h2>
          <button
            aria-label="Close source tracks"
            disabled={busy}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <p>
          {section.label} · Source {route.source} → destination{" "}
          {route.destination}. Changes apply to this section. Other sections
          keep their original rules.
        </p>
        <div className="source-body">
          <fieldset disabled={busy}>
            <button
              type="button"
              disabled={route.destination < 9 || route.destination > 16}
              onClick={() => {
                setBytes(partDefaultBytes(bytes));
                setMask(chords.map(() => true));
                setExclusive(false);
                setResetDefaults(true);
                setError(null);
              }}
            >
              Use part defaults
            </button>
            <p>
              Resets this draft’s CASM rules for destination {route.destination}
              , enables all chord roots and types, and keeps source notes and
              voice. Apply saves one undoable change; Cancel discards it.
            </p>
            <fieldset>
              <legend>Allowed chord roots (Note Mute)</legend>
              <p>
                Unchecked roots mute this source for that chord root, regardless
                of chord type. Source note pitches are not filtered.
              </p>
              <div className="source-fields">
                {roots.map((name, root) => (
                  <label key={root}>
                    <input
                      type="checkbox"
                      aria-label={`Allow chord root ${name}`}
                      checked={Boolean(
                        bytes[12 - Math.floor(root / 8)] & (1 << (root % 8)),
                      )}
                      onChange={(e) =>
                        update({
                          type: "rootEnabled",
                          root,
                          enabled: e.target.checked,
                        })
                      }
                    />
                    {name}
                  </label>
                ))}
              </div>
              <label>
                <input
                  type="checkbox"
                  aria-label="CASM Auto Start"
                  checked={Boolean(bytes[13] & 4)}
                  onChange={(e) =>
                    update({ type: "autoStart", enabled: e.target.checked })
                  }
                />
                Auto Start
              </label>
              <p>
                Allows a rhythm part before a chord is detected. Live
                progression preview starts with a chord; it uses the root and
                chord masks.
              </p>
            </fieldset>
            <div className="source-fields">
              <Choice
                label="Source root"
                value={bytes[18]}
                options={roots}
                onChange={(value) => update({ type: "root", value })}
              />
              <Choice
                label="Source chord"
                value={bytes[19]}
                options={chords}
                onChange={(value) => update({ type: "chord", value })}
              />
              <Numeric
                label="Middle zone low"
                value={bytes[20]}
                onChange={(low) =>
                  update({ type: "zone", low, high: bytes[21] })
                }
              />
              <Numeric
                label="Middle zone high"
                value={bytes[21]}
                onChange={(high) =>
                  update({ type: "zone", low: bytes[20], high })
                }
              />
            </div>
            <div
              className="source-regions"
              role="group"
              aria-label="Conversion region"
            >
              {(["low", "middle", "high"] as Region[]).map((r) => (
                <button
                  key={r}
                  aria-pressed={r === region}
                  onClick={() => setRegion(r)}
                >
                  {r[0].toUpperCase() + r.slice(1)} region
                </button>
              ))}
            </div>
            <p className="source-zone">
              Source pitches:{" "}
              {region === "low"
                ? bytes[20] === 0
                  ? "inactive"
                  : `0–${bytes[20] - 1}`
                : region === "high"
                  ? bytes[21] === 127
                    ? "inactive"
                    : `${bytes[21] + 1}–127`
                  : `${bytes[20]}–${bytes[21]}`}{" "}
              · Output limits are independent.
            </p>
            <div className="source-fields">
              <Choice
                label="Region NTR"
                value={bytes[o]}
                options={["Root Trans", "Root Fixed", "Guitar"]}
                onChange={(value) => rule("ntr", value)}
              />
              <Choice
                label="Region NTT"
                value={bytes[o + 1] & 127}
                options={
                  bytes[o] === 2
                    ? ["All Purpose", "Stroke", "Arpeggio"]
                    : bytes[o] <= 1
                      ? tables
                      : []
                }
                onChange={(value) => rule("ntt", value)}
              />
              <Choice
                label="Region high key"
                value={bytes[o + 2]}
                options={roots}
                onChange={(value) => rule("highKey", value)}
              />
              <Choice
                label="Region retrigger"
                value={bytes[o + 5]}
                options={[
                  "Stop",
                  "Pitch Shift",
                  "Pitch Shift to Root",
                  "Retrigger",
                  "Retrigger to Root",
                ]}
                onChange={(value) => rule("retrigger", value)}
              />
              <Numeric
                label="Output note low"
                value={bytes[o + 3]}
                onChange={(value) => rule("noteLow", value)}
              />
              <Numeric
                label="Output note high"
                value={bytes[o + 4]}
                onChange={(value) => rule("noteHigh", value)}
              />
            </div>
            <label className="source-check">
              <input
                type="checkbox"
                checked={Boolean(bytes[o + 1] & 128)}
                onChange={(e) => rule("bass", e.target.checked ? 1 : 0)}
              />{" "}
              NTT Bass
            </label>
            <details className="source-chords" open>
              <summary>
                Enabled chords · {mask.filter(Boolean).length} of 34
              </summary>
              <div className="source-groups">
                {[
                  ["Major", [0, 1, 2, 3, 4, 5, 6]],
                  ["Minor", [8, 9, 10, 11, 12, 13, 14, 15, 16]],
                ].map(([name, ids]) => (
                  <button
                    key={String(name)}
                    onClick={() => {
                      const members = ids as number[];
                      const on = !members.every((id) => mask[id]);
                      setMask((values) =>
                        values.map((value, id) =>
                          members.includes(id) ? on : value,
                        ),
                      );
                    }}
                  >
                    {name} group
                  </button>
                ))}
              </div>
              <div className="chord-grid">
                {chords.map((name, id) => (
                  <label key={id}>
                    <input
                      type="checkbox"
                      aria-label={`Enable chord ${name}`}
                      checked={mask[id]}
                      onChange={(e) =>
                        setMask((values) =>
                          values.map((value, i) =>
                            i === id ? e.target.checked : value,
                          ),
                        )
                      }
                    />
                    <span>{name}</span>
                  </label>
                ))}
              </div>
              <label className="source-check">
                <input
                  type="checkbox"
                  checked={exclusive}
                  onChange={(e) => setExclusive(e.target.checked)}
                />{" "}
                Assign enabled chords exclusively to this source
              </label>
              <p className="source-zone">
                Exclusive assignment clears these chords on sibling sources
                routed to destination {route.destination} in {section.label}.
              </p>
            </details>
            <label className="field">
              <span>Clone into source channel</span>
              <select
                aria-label="Clone into source channel"
                value={target}
                onChange={(e) => setTarget(Number(e.target.value))}
              >
                <option value={0}>Do not clone</option>
                {Array.from({ length: 16 }, (_, i) => i + 1).map((ch) => (
                  <option key={ch} value={ch} disabled={used.has(ch)}>
                    {ch}
                    {used.has(ch) ? " · in use" : ""}
                  </option>
                ))}
              </select>
            </label>
            {target > 0 && (
              <p className="source-zone">
                The copy includes this source’s notes, controllers, program and
                pitch bend. All 34 supported chord enables start off.
              </p>
            )}
          </fieldset>
        </div>
        {(invalid || error) && (
          <p role="alert" className="source-error">
            {error ??
              "Source zones or output limits are inverted or outside 0–127."}
          </p>
        )}
        <div className="modal-actions">
          <button disabled={busy} onClick={onClose}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={
              busy ||
              invalid ||
              (!changes.length && !maskChanged && !exclusive && !target)
            }
            onClick={() => void apply()}
          >
            {busy ? "Applying…" : "Apply source changes"}
          </button>
        </div>
      </section>
    </div>
  );
}
