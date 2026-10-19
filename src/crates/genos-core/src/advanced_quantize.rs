//! Pure timing plans. No MIDI payload, CASM, history, or document is mutated here.
//! Behavior was independently implemented from the observed SFM control contract.
use crate::{
    Error, Result,
    document::{Document, Note, Section},
    edit::NoteUpdate,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub const MAX_GRID_POINTS: usize = 100_000;
pub const MAX_ASSIGNMENT_CELLS: usize = 4_000_000;
pub const MAX_SELECTED_NOTES: usize = 20_000;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    #[default]
    Starts,
    Ends,
    Lengths,
    Both,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Yamaha,
    Relative,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Grid {
    #[serde(rename = "480")]
    Quarter,
    #[serde(rename = "320")]
    QuarterTriplet,
    #[serde(rename = "240")]
    Eighth,
    #[serde(rename = "160")]
    EighthTriplet,
    #[default]
    #[serde(rename = "120")]
    Sixteenth,
    #[serde(rename = "80")]
    SixteenthTriplet,
    #[serde(rename = "60")]
    ThirtySecond,
    #[serde(rename = "40")]
    ThirtySecondTriplet,
    #[serde(rename = "mixed8")]
    MixedEighth,
    #[serde(rename = "mixed16")]
    MixedSixteenth,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct QuantizeOptions {
    pub target: Target,
    pub mode: Mode,
    pub size: Grid,
    pub soft: bool,
    pub strength: f64,
    pub tuplet: u8,
    pub swing: f64,
    pub catch_enabled: bool,
    pub catch_percent: f64,
    pub safe_range: u16,
    pub rough: u16,
    pub move_controllers: bool,
    pub from_original: bool,
    pub source_filtered: bool,
}
impl Default for QuantizeOptions {
    fn default() -> Self {
        Self {
            target: Target::Starts,
            mode: Mode::Yamaha,
            size: Grid::Sixteenth,
            soft: true,
            strength: 100.,
            tuplet: 0,
            swing: 0.,
            catch_enabled: false,
            catch_percent: 100.,
            safe_range: 0,
            rough: 0,
            move_controllers: true,
            from_original: false,
            source_filtered: false,
        }
    }
}
impl QuantizeOptions {
    pub fn validate(&self) -> Result<()> {
        for (name, value, min, max) in [
            ("Strength", self.strength, 0., 100.),
            ("Swing", self.swing, 0., 100.),
            ("Catch", self.catch_percent, 1., 100.),
        ] {
            if !value.is_finite() || value < min || value > max {
                return fail(&format!("{name} must be finite and in {min}..={max}"));
            }
        }
        if ![0, 3, 5, 7].contains(&self.tuplet) || self.safe_range > 960 || self.rough > 120 {
            return fail("Tuplet, Safe, or Rough is outside its supported range");
        }
        if self.mode == Mode::Relative && self.target != Target::Starts {
            return fail("Group offset is only available for note starts");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteOriginal {
    pub id: u64,
    pub start: u64,
    pub duration: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventOriginal {
    pub id: u64,
    pub tick: u64,
    pub owner_note_id: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Baseline {
    pub notes: Vec<NoteOriginal>,
    pub events: Vec<EventOriginal>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventMove {
    pub id: u64,
    pub tick: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub notes: Vec<NoteUpdate>,
    pub events: Vec<EventMove>,
    pub baseline: Baseline,
}
impl Plan {
    pub fn is_noop(&self) -> bool {
        self.notes.is_empty() && self.events.is_empty()
    }
}
fn fail<T>(message: &str) -> Result<T> {
    Err(Error::Edit(format!("Advanced quantize: {message}")))
}

fn scope<'a>(
    doc: &'a Document,
    section: u64,
    channel: u8,
    ids: &[u64],
) -> Result<(&'a Section, Vec<&'a Note>)> {
    if !(9..=16).contains(&channel) {
        return fail("select a Yamaha source part (channel 9..=16)");
    }
    let sec = doc
        .sections
        .iter()
        .find(|s| s.id == section)
        .ok_or_else(|| Error::Edit("Quantize section not found".into()))?;
    if sec.end <= sec.start || sec.end > crate::smf::MAX_TICK {
        return fail("invalid section bounds");
    }
    let wanted: HashSet<_> = ids.iter().copied().collect();
    if wanted.len() != ids.len() {
        return fail("duplicate selected note IDs");
    }
    let notes: Vec<_> = doc
        .notes
        .iter()
        .filter(|n| {
            n.channel == channel
                && n.start >= sec.start
                && n.start < sec.end
                && (ids.is_empty() || wanted.contains(&n.id))
        })
        .collect();
    if !ids.is_empty() && notes.len() != wanted.len() {
        return fail("selected notes are missing or outside the source/section");
    }
    if notes.len() > MAX_SELECTED_NOTES {
        return fail("selected-note limit exceeded");
    }
    if notes
        .iter()
        .any(|n| n.duration == 0 || n.start.checked_add(n.duration).is_none_or(|e| e > sec.end))
    {
        return fail("a selected note crosses the section boundary");
    }
    Ok((sec, notes))
}
fn validate_baseline(b: &Baseline) -> Result<()> {
    let mut ids = HashSet::new();
    if b.notes.len() > MAX_SELECTED_NOTES * 16 || b.events.len() > 1_000_000 {
        return fail("baseline size limit exceeded");
    }
    for n in &b.notes {
        if !ids.insert(n.id)
            || n.duration == 0
            || n.start
                .checked_add(n.duration)
                .is_none_or(|e| e > crate::smf::MAX_TICK)
        {
            return fail("invalid or duplicate note baseline");
        }
    }
    ids.clear();
    for e in &b.events {
        if !ids.insert(e.id) || e.tick > crate::smf::MAX_TICK {
            return fail("invalid or duplicate controller baseline");
        }
    }
    Ok(())
}

/// Reserved project metadata is validated before it reaches the UI or planner.
/// A scope key binds its originals to one section and Yamaha source channel.
pub(crate) fn project_state(
    value: &serde_json::Value,
    doc: &Document,
    prune: bool,
) -> Result<Option<serde_json::Value>> {
    let mut states: std::collections::BTreeMap<String, Baseline> =
        serde_json::from_value(value.clone())?;
    if states.len() > doc.sections.len().saturating_mul(8) {
        return fail("too many original-timing scopes");
    }
    let notes: HashMap<_, _> = doc.notes.iter().map(|n| (n.id, n)).collect();
    let events: HashMap<_, _> = doc
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| (e.id, e))
        .collect();
    for (key, baseline) in &mut states {
        validate_baseline(baseline)?;
        let (section, channel) = key
            .split_once(':')
            .ok_or_else(|| Error::Edit("invalid quantize scope".into()))?;
        let section: u64 = section
            .parse()
            .map_err(|_| Error::Edit("invalid quantize section".into()))?;
        let channel: u8 = channel
            .parse()
            .map_err(|_| Error::Edit("invalid quantize channel".into()))?;
        if *key != format!("{section}:{channel}") || !(9..=16).contains(&channel) {
            return fail("invalid original-timing scope");
        }
        let sec = doc.sections.iter().find(|s| s.id == section);
        let note_in_scope = |id: u64| {
            notes.get(&id).is_some_and(|n| {
                n.channel == channel && sec.is_some_and(|s| n.start >= s.start && n.start < s.end)
            })
        };
        let valid_note = |n: &NoteOriginal| {
            note_in_scope(n.id)
                && sec.is_some_and(|s| {
                    n.start >= s.start
                        && n.start
                            .checked_add(n.duration)
                            .is_some_and(|end| end <= s.end)
                })
        };
        let valid_event = |e: &EventOriginal| {
            note_in_scope(e.owner_note_id)
                && sec.is_some_and(|s| {
                    e.tick >= s.start
                        && e.tick < s.end
                        && events.get(&e.id).is_some_and(|current| {
                            movable(current, channel)
                                && current.tick >= s.start
                                && current.tick < s.end
                        })
                })
        };
        if prune {
            baseline.notes.retain(valid_note);
            baseline.events.retain(valid_event);
        } else if baseline.notes.iter().any(|n| !valid_note(n))
            || baseline.events.iter().any(|e| !valid_event(e))
        {
            return fail("original timing is not bound to its section/part");
        }
    }
    states.retain(|_, b| !b.notes.is_empty() || !b.events.is_empty());
    Ok(if states.is_empty() {
        None
    } else {
        Some(serde_json::to_value(states)?)
    })
}
fn steps(doc: &Document, o: &QuantizeOptions) -> Vec<u64> {
    let refs: &[u64] = match o.size {
        Grid::Quarter => &[480],
        Grid::QuarterTriplet => &[320],
        Grid::Eighth => &[240],
        Grid::EighthTriplet => &[160],
        Grid::Sixteenth => &[120],
        Grid::SixteenthTriplet => &[80],
        Grid::ThirtySecond => &[60],
        Grid::ThirtySecondTriplet => &[40],
        Grid::MixedEighth => &[240, 160],
        Grid::MixedSixteenth => &[120, 80],
    };
    // Round once after rational PPQ and tuplet scaling, including odd PPQ.
    let mut out = Vec::new();
    for r in refs {
        let n = r
            * doc.style.ppq.max(24) as u64
            * if o.tuplet > 0 && refs.len() == 1 {
                4
            } else {
                1
            };
        let d = 480
            * if o.tuplet > 0 && refs.len() == 1 {
                o.tuplet as u64
            } else {
                1
            };
        let s = ((n + d / 2) / d).max(1);
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}
fn grid_tick(i: u64, step: u64, swing: f64) -> u64 {
    i * step
        + if i % 2 == 1 {
            (step as f64 * swing / 300.).round() as u64
        } else {
            0
        }
}
fn candidates(steps: &[u64], limit: u64, swing: f64) -> Result<Vec<u64>> {
    if steps.iter().map(|s| limit / s + 1).sum::<u64>() > MAX_GRID_POINTS as u64 {
        return fail("grid-point limit exceeded; choose a coarser grid or shorter section");
    }
    let mut set = BTreeSet::from([0]);
    for step in steps {
        for i in 0..=limit / step {
            let t = grid_tick(i, *step, if steps.len() == 1 { swing } else { 0. });
            if t <= limit {
                set.insert(t);
            }
        }
    }
    Ok(set.into_iter().collect())
}
fn nearest(t: u64, steps: &[u64], swing: f64) -> u64 {
    let mut best = None;
    for step in steps {
        let center = (t + step / 2) / step;
        for i in center.saturating_sub(2)..=center + 2 {
            let v = grid_tick(i, *step, if steps.len() == 1 { swing } else { 0. });
            if best.is_none_or(|b: u64| v.abs_diff(t) < b.abs_diff(t)) {
                best = Some(v);
            }
        }
    }
    best.unwrap_or(0)
}
fn allowed(current: u64, target: u64, o: &QuantizeOptions, step: u64) -> bool {
    let d = current.abs_diff(target);
    !(o.safe_range > 0 && d <= o.safe_range as u64
        || o.catch_enabled && d as f64 > step as f64 * 0.5 * o.catch_percent / 100.)
}
fn rough(n: &Note, original: u64, amount: u16) -> i64 {
    if amount == 0 {
        return 0;
    }
    let identity = format!("{}:{}:{}", n.id, original, n.pitch);
    let hash = identity
        .encode_utf16()
        .fold(2166136261u32, |h, c| (h ^ c as u32).wrapping_mul(16777619));
    (hash % (2 * amount as u32 + 1)) as i64 - amount as i64
}
fn interpolate(current: u64, target: u64, strength: f64, noise: i64, min: u64, max: u64) -> u64 {
    // ECMAScript Math.round rounds ties toward +infinity, including negative offsets.
    (current as f64 + (target as f64 - current as f64) * strength / 100. + noise as f64 + 0.5)
        .floor()
        .clamp(min as f64, max as f64) as u64
}
fn nearest_endpoint(grid: &[u64], current: u64, min: u64, max: u64) -> u64 {
    let lo = grid.partition_point(|g| *g < min);
    let hi = grid.partition_point(|g| *g <= max);
    if lo == hi {
        return max;
    }
    let valid = &grid[lo..hi];
    let i = valid.partition_point(|g| *g < current);
    match (i.checked_sub(1).map(|j| valid[j]), valid.get(i).copied()) {
        (Some(a), Some(b)) => {
            if a.abs_diff(current) <= b.abs_diff(current) {
                a
            } else {
                b
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => max,
    }
}
#[derive(Clone)]
struct Working<'a> {
    note: &'a Note,
    start: u64,
    duration: u64,
    from: u64,
    end: u64,
    original: u64,
}

fn assignments(
    notes: &[Working<'_>],
    grid: &[u64],
    length: u64,
    both: bool,
    steps: &[u64],
    swing: f64,
) -> Result<HashMap<u64, u64>> {
    let mut lanes: BTreeMap<u8, BTreeMap<u64, Vec<&Working<'_>>>> = BTreeMap::new();
    for n in notes {
        lanes
            .entry(n.note.pitch)
            .or_default()
            .entry(n.start)
            .or_default()
            .push(n);
    }
    let mut out = HashMap::new();
    let mut total = 0usize;
    for lane in lanes.values() {
        let clusters: Vec<_> = lane.iter().collect();
        let width = grid.len();
        let cells = clusters
            .len()
            .checked_mul(width)
            .ok_or_else(|| Error::Edit("Quantize assignment too large".into()))?;
        total = total.saturating_add(cells);
        if total > MAX_ASSIGNMENT_CELLS {
            return fail("assignment work limit exceeded; shorten the selection");
        }
        if clusters.len() > width {
            for (tick, cluster) in &clusters {
                for n in *cluster {
                    out.insert(n.note.id, nearest(**tick, steps, swing));
                }
            }
            continue;
        }
        let mut cost = vec![u64::MAX; width];
        let mut links = vec![u32::MAX; cells];
        for (row, (tick, cluster)) in clusters.iter().enumerate() {
            let max = cluster
                .iter()
                .map(|n| length - if both { 1 } else { n.duration })
                .min()
                .unwrap();
            let mut next = vec![u64::MAX; width];
            let mut best = (u64::MAX, u32::MAX);
            for (col, g) in grid.iter().enumerate() {
                if row == 0 {
                    if *g <= max {
                        next[col] = g.abs_diff(**tick);
                    }
                    continue;
                }
                if col > 0 && cost[col - 1] < best.0 {
                    best = (cost[col - 1], (col - 1) as u32);
                }
                if *g <= max && best.0 != u64::MAX {
                    next[col] = best.0 + g.abs_diff(**tick);
                    links[row * width + col] = best.1;
                }
            }
            cost = next;
        }
        let Some((mut col, _)) = cost
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != u64::MAX)
            .min_by_key(|(_, v)| **v)
        else {
            for (tick, cluster) in &clusters {
                for n in *cluster {
                    out.insert(n.note.id, nearest(**tick, steps, swing));
                }
            }
            continue;
        };
        for row in (0..clusters.len()).rev() {
            for n in clusters[row].1 {
                out.insert(n.note.id, grid[col]);
            }
            if row > 0 {
                col = links[row * width + col] as usize;
            }
        }
    }
    Ok(out)
}

/// Produce a plan against immutable authoritative timing. IDs are explicit, not indices.
pub fn plan(
    doc: &Document,
    section: u64,
    channel: u8,
    ids: &[u64],
    o: &QuantizeOptions,
    baseline: Option<&Baseline>,
) -> Result<Plan> {
    o.validate()?;
    let (sec, selected) = scope(doc, section, channel, ids)?;
    let length = sec.end - sec.start;
    let prior = baseline.cloned().unwrap_or_default();
    validate_baseline(&prior)?;
    let originals: HashMap<_, _> = prior.notes.iter().map(|n| (n.id, n)).collect();
    let mut work = Vec::new();
    for n in &selected {
        let saved = originals.get(&n.id);
        let original = saved.map_or(n.start, |b| b.start);
        if original < sec.start || original >= sec.end {
            return fail("saved original onset belongs to another section");
        }
        let (mut start, mut duration) = (n.start, n.duration);
        if o.from_original
            && let Some(b) = saved
        {
            match o.target {
                Target::Starts => start = b.start,
                Target::Ends => duration = (b.start + b.duration).saturating_sub(start).max(1),
                Target::Lengths => duration = b.duration,
                Target::Both => {
                    start = b.start;
                    duration = b.duration;
                }
            }
        }
        if start < sec.start || start >= sec.end || duration > sec.end - start {
            return fail("saved note timing is outside this section");
        }
        work.push(Working {
            note: n,
            start: start - sec.start,
            duration,
            from: start - sec.start,
            end: if o.from_original && o.target == Target::Ends {
                saved.map_or(start + duration, |b| b.start + b.duration) - sec.start
            } else {
                start + duration - sec.start
            },
            original: original - sec.start,
        });
    }
    if work.is_empty() {
        return Ok(Plan {
            notes: vec![],
            events: vec![],
            baseline: prior,
        });
    }
    let steps = steps(doc, o);
    let grid = candidates(&steps, length, o.swing)?;
    let strength = if o.soft { o.strength } else { 100. };
    let moves_starts = matches!(o.target, Target::Starts | Target::Both);
    if moves_starts {
        if o.mode == Mode::Relative {
            let anchor = work.iter().map(|n| n.start).min().unwrap();
            let to = nearest(anchor, &steps, o.swing);
            let mut delta = if allowed(anchor, to, o, *steps.iter().min().unwrap()) {
                ((to as f64 - anchor as f64) * strength / 100. + 0.5).floor() as i64
                    + rough(work[0].note, work[0].original, o.rough)
            } else {
                0
            };
            let end = work.iter().map(|n| n.start + n.duration).max().unwrap();
            delta = delta.clamp(-(anchor as i64), (length - end) as i64);
            for n in &mut work {
                n.start = (n.start as i64 + delta) as u64;
            }
        } else {
            let targets = assignments(
                &work,
                &grid,
                length,
                o.target == Target::Both,
                &steps,
                o.swing,
            )?;
            for n in &mut work {
                let to = targets[&n.note.id];
                if allowed(n.start, to, o, *steps.iter().min().unwrap()) {
                    n.start = interpolate(
                        n.start,
                        to,
                        strength,
                        rough(n.note, n.original, o.rough),
                        0,
                        length
                            - if o.target == Target::Both {
                                1
                            } else {
                                n.duration
                            },
                    );
                }
            }
            // Partial strength / roughness can collide after a valid grid assignment.
            let mut order: Vec<_> = (0..work.len()).collect();
            order.sort_by_key(|i| (work[*i].note.pitch, work[*i].from, work[*i].note.id));
            let mut last: Option<(u8, u64, u64)> = None;
            for i in order {
                let n = &mut work[i];
                if let Some((pitch, from, start)) = last
                    && pitch == n.note.pitch
                    && from < n.from
                    && n.start <= start
                {
                    n.start = start + 1;
                }
                n.start = n.start.min(
                    length
                        - if o.target == Target::Both {
                            1
                        } else {
                            n.duration
                        },
                );
                last = Some((n.note.pitch, n.from, n.start));
            }
        }
    }
    if o.target != Target::Starts {
        let length_grid = if o.target == Target::Lengths {
            candidates(&steps, length, 0.)?
        } else {
            vec![]
        };
        for n in &mut work {
            let lengths = o.target == Target::Lengths;
            let current = if lengths { n.duration } else { n.end };
            let (min, max) = if lengths {
                (1, length - n.start)
            } else {
                (n.start + 1, length)
            };
            let endpoints = if lengths { &length_grid } else { &grid };
            let to = nearest_endpoint(endpoints, current, min, max);
            let value = if allowed(current, to, o, *steps.iter().min().unwrap()) {
                interpolate(
                    current,
                    to,
                    strength,
                    rough(n.note, n.original, o.rough),
                    min,
                    max,
                )
            } else {
                current.clamp(min, max)
            };
            n.duration = if lengths { value } else { value - n.start };
        }
    }
    let mut result = Plan {
        notes: vec![],
        events: vec![],
        baseline: prior.clone(),
    };
    for n in &work {
        let start = sec.start + n.start;
        if start != n.note.start || n.duration != n.note.duration {
            result.notes.push(NoteUpdate {
                id: n.note.id,
                start,
                duration: n.duration,
                pitch: n.note.pitch,
                velocity: n.note.velocity,
            });
        }
    }
    if o.move_controllers && moves_starts && !o.source_filtered {
        let eq_ids = eq_packet_ids(doc, channel);
        let mut ordered: Vec<_> = work.iter().collect();
        ordered.sort_by_key(|n| (n.from, n.note.pitch));
        let first = ordered[0].from;
        let end = ordered
            .iter()
            .map(|n| n.from + (n.end - n.from))
            .max()
            .unwrap();
        let owners: HashSet<_> = selected.iter().map(|n| n.id).collect();
        let old_events: HashMap<_, _> = prior
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id, i))
            .collect();
        for e in doc.style.tracks.iter().flatten().filter(|e| {
            movable(e, channel)
                && !eq_ids.contains(&e.id)
                && e.tick >= sec.start
                && e.tick < sec.end
        }) {
            let saved = old_events.get(&e.id).map(|i| &prior.events[*i]);
            let tick = if o.from_original {
                saved
                    .filter(|b| owners.contains(&b.owner_note_id))
                    .map_or(e.tick, |b| b.tick)
            } else {
                e.tick
            };
            let mut to = tick;
            if tick < sec.start || tick >= sec.end {
                return fail("saved controller timing belongs to another section");
            }
            if tick > sec.start
                && tick >= sec.start + first
                && tick < sec.start + end
                && let Some(owner) = ordered
                    .partition_point(|n| n.from <= tick - sec.start)
                    .checked_sub(1)
                    .map(|i| ordered[i])
            {
                let delta = owner.start as i64 - owner.from as i64;
                if delta != 0 {
                    to = (tick as i64 + delta).clamp(sec.start as i64, sec.end as i64 - 1) as u64;
                    if saved.is_none() {
                        result.baseline.events.push(EventOriginal {
                            id: e.id,
                            tick: e.tick,
                            owner_note_id: owner.note.id,
                        });
                    } else {
                        result.baseline.events[old_events[&e.id]].owner_note_id = owner.note.id;
                    }
                }
            }
            if to != e.tick {
                result.events.push(EventMove { id: e.id, tick: to });
            }
        }
    }
    if result.is_noop() {
        result.baseline = prior;
        return Ok(result);
    }
    for n in selected {
        if !originals.contains_key(&n.id) {
            result.baseline.notes.push(NoteOriginal {
                id: n.id,
                start: n.start,
                duration: n.duration,
            });
        }
    }
    validate_fifo(doc, &result.notes)?;
    Ok(result)
}
fn movable(e: &crate::smf::Event, channel: u8) -> bool {
    if e.data.len() != 3 || e.channel() != Some(channel) {
        return false;
    }
    match e.data[0] & 0xf0 {
        0xe0 => true,
        0xb0 => !matches!(e.data[1], 0 | 32 | 120..=127),
        _ => false,
    }
}

// EQ automation uses CC transport bytes but is a separate typed operation.
// Decode only complete recognized packets; unrelated interleaved CCs stay
// eligible. Group first so packet recognition never rescans a whole track
// for every event tick on a dense automation lane.
fn eq_packet_ids(doc: &Document, channel: u8) -> HashSet<u64> {
    use crate::channel_eq::{self, EqBand};
    let mut ids = HashSet::new();
    for track in &doc.style.tracks {
        let mut groups: BTreeMap<u64, Vec<crate::smf::Event>> = BTreeMap::new();
        for e in track.iter().filter(|e| {
            e.channel() == Some(channel)
                && e.data.len() == 3
                && e.data[0] & 0xf0 == 0xb0
                && matches!(e.data[1], 99 | 98 | 6 | 101 | 100)
        }) {
            groups.entry(e.tick).or_default().push(e.clone());
        }
        for (tick, mut events) in groups {
            events.sort_by_key(|e| (e.order, e.id));
            for band in [EqBand::EqLow, EqBand::EqHigh] {
                for i in channel_eq::packet_indices(&events, channel, tick, band) {
                    ids.insert(events[i].id);
                }
            }
        }
    }
    ids
}

/// Restore selected originals, preserving all baselines outside this operation.
pub fn reset(
    doc: &Document,
    section: u64,
    channel: u8,
    ids: &[u64],
    baseline: &Baseline,
    source_filtered: bool,
) -> Result<Plan> {
    validate_baseline(baseline)?;
    let (sec, notes) = scope(doc, section, channel, ids)?;
    let selected: HashSet<_> = notes.iter().map(|n| n.id).collect();
    let mut result = Plan {
        notes: vec![],
        events: vec![],
        baseline: baseline.clone(),
    };
    for n in notes {
        if let Some(b) = baseline.notes.iter().find(|b| b.id == n.id) {
            if b.start < sec.start || b.start + b.duration > sec.end {
                return fail("reset timing is outside section");
            }
            if b.start != n.start || b.duration != n.duration {
                result.notes.push(NoteUpdate {
                    id: n.id,
                    start: b.start,
                    duration: b.duration,
                    pitch: n.pitch,
                    velocity: n.velocity,
                });
            }
        }
    }
    result.baseline.notes.retain(|n| !selected.contains(&n.id));
    if !source_filtered {
        let eq_ids = eq_packet_ids(doc, channel);
        let events: HashMap<_, _> = doc
            .style
            .tracks
            .iter()
            .flatten()
            .map(|e| (e.id, e))
            .collect();
        let mut remove = HashSet::new();
        for b in &baseline.events {
            if (ids.is_empty() || selected.contains(&b.owner_note_id))
                && let Some(e) = events.get(&b.id)
                && movable(e, channel)
                && !eq_ids.contains(&e.id)
                && e.tick >= sec.start
                && e.tick < sec.end
            {
                if b.tick < sec.start || b.tick >= sec.end {
                    return fail("reset controller timing is outside section");
                }
                if b.tick != e.tick {
                    result.events.push(EventMove {
                        id: e.id,
                        tick: b.tick,
                    });
                }
                remove.insert(e.id);
            }
        }
        result.baseline.events.retain(|e| !remove.contains(&e.id));
    }
    validate_fifo(doc, &result.notes)?;
    Ok(result)
}

// Re-run FIFO on/off identity pairing, including untouched neighbours. Reject
// ambiguous transformations instead of silently exchanging note-off identities.
fn validate_fifo(doc: &Document, updates: &[NoteUpdate]) -> Result<()> {
    let changed: HashMap<_, _> = updates.iter().map(|u| (u.id, u)).collect();
    let affected: HashSet<_> = doc
        .notes
        .iter()
        .filter(|n| changed.contains_key(&n.id))
        .map(|n| (n.track, n.channel, n.pitch))
        .collect();
    let mut edges = HashMap::new();
    let original_ticks: HashMap<_, _> = doc
        .style
        .tracks
        .iter()
        .flatten()
        .map(|e| (e.id, e.tick))
        .collect();
    for n in &doc.notes {
        if let Some(u) = changed.get(&n.id) {
            edges.insert(n.id, u.start);
            let off = if u.duration == n.duration {
                // Document normalizes a zero-tick pair to one display tick.
                // The edit transaction translates its actual original off edge.
                let actual = original_ticks
                    .get(&n.off_id)
                    .copied()
                    .unwrap_or(n.start + n.duration);
                actual.saturating_add_signed(u.start as i64 - n.start as i64)
            } else {
                u.start + u.duration
            };
            edges.insert(n.off_id, off);
        }
    }
    let expected: HashMap<_, _> = doc.notes.iter().map(|n| (n.off_id, n.id)).collect();
    for (track, channel, pitch) in affected {
        let mut lane: Vec<_> = doc.style.tracks[track]
            .iter()
            .filter(|e| {
                e.data.len() == 3
                    && e.channel() == Some(channel)
                    && e.data[1] == pitch
                    && (e.is_on() || e.is_off())
            })
            .collect();
        lane.sort_by_key(|e| (edges.get(&e.id).copied().unwrap_or(e.tick), e.order));
        let mut pending = VecDeque::new();
        for e in lane {
            if e.is_on() {
                pending.push_back(e.id);
            } else if pending.pop_front() != expected.get(&e.id).copied() {
                return fail(
                    "timing would change MIDI FIFO note pairing; adjust selection or grid",
                );
            }
        }
    }
    Ok(())
}
