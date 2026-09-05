//! Client-only remote entity interpolation (Phase 5.3).
//!
//! Presentation layer separate from [`crate::replica::ReplicatedWorld`].
//! Does not modify authoritative replica state, input, or the server.

use std::collections::VecDeque;
use std::time::Instant;

use purgatory_protocol::{ReplicatedKind, WireEntityId, WorldSnapshot};
use purgatory_simulation::{TICK_DURATION, TICK_DURATION_NANOS};

/// Snapshot intervals to lag the render timeline behind estimated server time.
/// Three intervals ≈ 100 ms at 30 Hz — enough jitter room without extrapolating.
pub const INTERPOLATION_DELAY_TICKS: u64 = 3;

/// Bounded history depth (~0.5 s at 30 Hz). Evicts oldest.
pub const INTERPOLATION_HISTORY_CAP: usize = 16;

/// Presentation discontinuity threshold (world units). Dev tuning only — not a
/// gameplay rule. Larger bracket deltas snap to B instead of lerping.
pub const INTERPOLATION_SNAP_DISTANCE: f32 = 8.0;

#[must_use]
pub fn interpolation_delay_ms() -> u64 {
    INTERPOLATION_DELAY_TICKS.saturating_mul(TICK_DURATION.as_millis() as u64)
}

#[derive(Clone, Copy, Debug)]
struct SampleEntity {
    entity_id: WireEntityId,
    position: [f32; 2],
}

#[derive(Clone, Debug)]
struct HistorySample {
    #[allow(dead_code)]
    sequence: u32,
    server_tick: u64,
    local_player_entity: WireEntityId,
    entities: Vec<SampleEntity>,
}

impl HistorySample {
    fn from_snapshot(snap: &WorldSnapshot) -> Self {
        Self {
            sequence: snap.snapshot_sequence,
            server_tick: snap.server_tick,
            local_player_entity: snap.local_player_entity,
            entities: snap
                .entities
                .iter()
                .filter(|e| e.kind == ReplicatedKind::Player || e.kind == ReplicatedKind::Npc)
                .map(|e| SampleEntity {
                    entity_id: e.entity_id,
                    position: e.position,
                })
                .collect(),
        }
    }

    fn find(&self, id: WireEntityId) -> Option<[f32; 2]> {
        self.entities
            .iter()
            .find(|e| e.entity_id == id)
            .map(|e| e.position)
    }

    fn same_remote_entities(&self, other: &Self) -> bool {
        fn remotes(sample: &HistorySample) -> impl Iterator<Item = (WireEntityId, [f32; 2])> + '_ {
            sample.entities.iter().filter_map(|e| {
                (e.entity_id != sample.local_player_entity).then_some((e.entity_id, e.position))
            })
        }
        remotes(self).eq(remotes(other))
    }
}

/// One remote entity's presentation pose for the current frame.
#[derive(Clone, Copy, Debug)]
pub struct PresentationPose {
    #[allow(dead_code)] // matched in tests / future gizmos
    pub entity_id: WireEntityId,
    pub position: [f32; 2],
}

/// Lightweight diagnostics for the Network debug tab.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpDiagnostics {
    pub enabled: bool,
    pub delay_ticks: u64,
    pub delay_ms: u64,
    pub history_depth: u32,
    pub estimated_server_tick: f64,
    pub render_tick: f64,
    pub bracket_a_tick: Option<u64>,
    pub bracket_b_tick: Option<u64>,
    pub alpha: f32,
    pub holds: u64,
    pub snaps: u64,
    /// Duplicate remote-pose samples skipped. Counted; Network-tab surface may follow.
    #[allow(dead_code)]
    pub dup_skips: u64,
    pub oldest_tick: Option<u64>,
    pub newest_tick: Option<u64>,
    pub clamped_oldest: bool,
    pub clamped_newest: bool,
    pub reseeds: u64,
}

/// Bounded snapshot history + monotonic server-tick clock for remote lerp.
#[derive(Debug)]
pub struct InterpolationBuffer {
    history: VecDeque<HistorySample>,
    estimated_server_tick: f64,
    last_render_tick: f64,
    last_advance_at: Option<Instant>,
    poses: Vec<PresentationPose>,
    diag: InterpDiagnostics,
    holds: u64,
    snaps: u64,
    /// Cached last sample alpha / brackets for diagnostics.
    last_alpha: f32,
    last_bracket_a: Option<u64>,
    last_bracket_b: Option<u64>,
    dup_skips: u64,
    reseeds: u64,
}

impl Default for InterpolationBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl InterpolationBuffer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(INTERPOLATION_HISTORY_CAP),
            estimated_server_tick: 0.0,
            last_render_tick: 0.0,
            last_advance_at: None,
            poses: Vec::new(),
            diag: InterpDiagnostics {
                enabled: true,
                delay_ticks: INTERPOLATION_DELAY_TICKS,
                delay_ms: interpolation_delay_ms(),
                ..InterpDiagnostics::default()
            },
            holds: 0,
            snaps: 0,
            last_alpha: 0.0,
            last_bracket_a: None,
            last_bracket_b: None,
            dup_skips: 0,
            reseeds: 0,
        }
    }

    /// Push a replica-accepted authoritative snapshot. Late arrivals never
    /// rewind the estimated clock (forward catch-up only).
    pub fn push(&mut self, snap: &WorldSnapshot) {
        self.push_at(snap, Instant::now());
    }

    fn push_at(&mut self, snap: &WorldSnapshot, now: Instant) {
        let tick = snap.server_tick as f64;
        if self.last_advance_at.is_none() {
            self.estimated_server_tick = tick;
            self.last_advance_at = Some(now);
            self.last_render_tick = (tick - INTERPOLATION_DELAY_TICKS as f64).max(0.0);
        } else if tick > self.estimated_server_tick {
            // Snapshot catch-up already accounts for time since the last sample.
            // Do not also add that interval in `advance_clock` (permanent underrun).
            self.estimated_server_tick = tick;
            self.last_advance_at = Some(now);
        }

        let sample = HistorySample::from_snapshot(snap);
        // Delta replication + every-frame snapshot views repeat the last remote
        // pose on ticks with no transform update. Keeping those samples collapses
        // lerp onto a 1-tick jump (hold, then step). Skip the duplicates; keep
        // the earlier tick as the hold start.
        // Only consecutive ticks: those are replica-copied stale remotes on
        // frames that still arrived for the local player. Longer gaps are real
        // cadence/idle samples and must keep the latest hold tick.
        if self.history.back().is_some_and(|h| {
            h.server_tick.saturating_add(1) == sample.server_tick && h.same_remote_entities(&sample)
        }) {
            self.dup_skips = self.dup_skips.saturating_add(1);
            self.diag.dup_skips = self.dup_skips;
            return;
        }

        if self
            .history
            .back()
            .is_some_and(|h| h.server_tick > snap.server_tick)
        {
            // Out-of-order by tick: still store if newer sequence was accepted
            // by replica; keep history sorted by server_tick.
            let idx = self
                .history
                .iter()
                .position(|h| h.server_tick >= sample.server_tick)
                .unwrap_or(self.history.len());
            if idx < self.history.len() && self.history[idx].server_tick == sample.server_tick {
                self.history[idx] = sample;
            } else {
                self.history.insert(idx, sample);
            }
        } else if self
            .history
            .back()
            .is_some_and(|h| h.server_tick == snap.server_tick)
        {
            if let Some(back) = self.history.back_mut() {
                *back = sample;
            }
        } else {
            self.history.push_back(sample);
        }

        while self.history.len() > INTERPOLATION_HISTORY_CAP {
            self.history.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.reseeds = self.reseeds.saturating_add(1);
        self.history.clear();
        self.estimated_server_tick = 0.0;
        self.last_render_tick = 0.0;
        self.last_advance_at = None;
        self.poses.clear();
        self.holds = 0;
        self.snaps = 0;
        self.dup_skips = 0;
        self.last_alpha = 0.0;
        self.last_bracket_a = None;
        self.last_bracket_b = None;
        self.diag = InterpDiagnostics {
            enabled: true,
            delay_ticks: INTERPOLATION_DELAY_TICKS,
            delay_ms: interpolation_delay_ms(),
            reseeds: self.reseeds,
            ..InterpDiagnostics::default()
        };
    }

    #[cfg(test)]
    #[must_use]
    pub fn depth(&self) -> usize {
        self.history.len()
    }

    #[must_use]
    pub fn diagnostics(&self) -> InterpDiagnostics {
        self.diag
    }

    #[must_use]
    pub fn poses(&self) -> &[PresentationPose] {
        &self.poses
    }

    /// How many history samples currently contain `entity_id` (and the tick span).
    #[must_use]
    pub fn remote_history_span(&self, entity_id: WireEntityId) -> (u32, Option<u64>, Option<u64>) {
        let mut count = 0u32;
        let mut oldest = None;
        let mut newest = None;
        for sample in &self.history {
            if sample.find(entity_id).is_some() {
                count = count.saturating_add(1);
                if oldest.is_none() {
                    oldest = Some(sample.server_tick);
                }
                newest = Some(sample.server_tick);
            }
        }
        (count, oldest, newest)
    }

    /// Distinct-position ticks for one remote (padded cadence copies collapsed).
    #[must_use]
    pub fn remote_unique_transform_span(&self, entity_id: WireEntityId) -> EntityTransformSpan {
        entity_transform_span(&self.history, entity_id)
    }

    /// Per-entity lerp bracket at the current render tick (padded poses ignored).
    #[must_use]
    pub fn remote_entity_bracket(&self, entity_id: WireEntityId) -> EntityInterpBracket {
        entity_interp_bracket(&self.history, entity_id, self.diag.render_tick)
    }

    /// Advance the monotonic clock and fill remote presentation poses.
    /// Local player is never included.
    pub fn sample(&mut self, now: Instant) {
        self.advance_clock(now);
        let render = self.render_tick();
        self.poses.clear();

        if self.history.is_empty() {
            self.refresh_diag(render, None, None, 0.0);
            return;
        }

        if self.history.len() < 2 {
            let (tick, local, entities): (u64, WireEntityId, Vec<SampleEntity>) = {
                let latest = self.history.back().expect("non-empty");
                (
                    latest.server_tick,
                    latest.local_player_entity,
                    latest.entities.clone(),
                )
            };
            for entity in &entities {
                if entity.entity_id == local {
                    continue;
                }
                self.poses.push(PresentationPose {
                    entity_id: entity.entity_id,
                    position: entity.position,
                });
            }
            self.refresh_diag(render, Some(tick), Some(tick), 0.0);
            return;
        }

        let (a_idx, b_idx, alpha, held) = find_bracket(&self.history, render);
        if held {
            self.holds = self.holds.saturating_add(1);
        }
        let a = &self.history[a_idx];
        let b = &self.history[b_idx];
        self.last_alpha = alpha;
        self.last_bracket_a = Some(a.server_tick);
        self.last_bracket_b = Some(b.server_tick);

        // Presence: when render has reached B, use B's entity set. Until then
        // keep A's entities that B removed (despawn waits for render timeline).
        let past_b = render >= b.server_tick as f64;
        let local = b.local_player_entity;

        if past_b || a_idx == b_idx {
            // At or past B (or identical samples): presence from B.
            for entity in &b.entities {
                if entity.entity_id == local {
                    continue;
                }
                let pos = interp_entity_pose(
                    &self.history,
                    entity.entity_id,
                    render,
                    entity.position,
                    &mut self.snaps,
                );
                self.poses.push(PresentationPose {
                    entity_id: entity.entity_id,
                    position: pos,
                });
            }
        } else {
            // Between A and B: draw remotes present in A; new-in-B stay hidden
            // until render reaches B; A-only stay until then.
            for entity in &a.entities {
                if entity.entity_id == local {
                    continue;
                }
                let pos = if b.find(entity.entity_id).is_some() {
                    interp_entity_pose(
                        &self.history,
                        entity.entity_id,
                        render,
                        entity.position,
                        &mut self.snaps,
                    )
                } else {
                    // Despawn pending: hold A until render passes B.
                    entity.position
                };
                self.poses.push(PresentationPose {
                    entity_id: entity.entity_id,
                    position: pos,
                });
            }
        }

        self.refresh_diag(render, Some(a.server_tick), Some(b.server_tick), alpha);
    }

    fn advance_clock(&mut self, now: Instant) {
        let Some(last) = self.last_advance_at else {
            return;
        };
        let elapsed = now.saturating_duration_since(last);
        let ticks = elapsed.as_nanos() as f64 / TICK_DURATION_NANOS as f64;
        self.estimated_server_tick += ticks;
        self.last_advance_at = Some(now);
    }

    fn render_tick(&mut self) -> f64 {
        let candidate = self.estimated_server_tick - INTERPOLATION_DELAY_TICKS as f64;
        let render = candidate.max(self.last_render_tick);
        self.last_render_tick = render;
        render
    }

    fn refresh_diag(&mut self, render: f64, a: Option<u64>, b: Option<u64>, alpha: f32) {
        let oldest = self.history.front().map(|h| h.server_tick);
        let newest = self.history.back().map(|h| h.server_tick);
        let single = self.history.len() < 2 && newest.is_some();
        self.diag = InterpDiagnostics {
            enabled: true,
            delay_ticks: INTERPOLATION_DELAY_TICKS,
            delay_ms: interpolation_delay_ms(),
            history_depth: self.history.len() as u32,
            estimated_server_tick: self.estimated_server_tick,
            render_tick: render,
            bracket_a_tick: a,
            bracket_b_tick: b,
            alpha,
            holds: self.holds,
            snaps: self.snaps,
            dup_skips: self.dup_skips,
            oldest_tick: oldest,
            newest_tick: newest,
            clamped_oldest: oldest.is_some_and(|tick| render <= tick as f64),
            clamped_newest: single || newest.is_some_and(|tick| render >= tick as f64),
            reseeds: self.reseeds,
        };
    }

    /// Test / debug: estimated server tick after last sample.
    #[cfg(test)]
    #[must_use]
    pub fn estimated_server_tick(&self) -> f64 {
        self.estimated_server_tick
    }

    #[cfg(test)]
    #[must_use]
    pub fn last_render_tick(&self) -> f64 {
        self.last_render_tick
    }
}

/// Distinct-position history for one remote (cadence-padded copies removed).
#[derive(Clone, Copy, Debug, Default)]
pub struct EntityTransformSpan {
    pub unique_count: u32,
    pub oldest_tick: Option<u64>,
    pub newest_tick: Option<u64>,
    pub last_gap: Option<u64>,
    pub ticks: [u64; 8],
    pub ticks_len: u8,
}

/// Per-entity interpolation bracket at a render tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct EntityInterpBracket {
    pub sample_a: Option<u64>,
    pub sample_b: Option<u64>,
    pub alpha: f32,
    pub clamped_newest: bool,
}

/// Interpolated remote pose if present; otherwise the latest replica fallback.
#[must_use]
pub fn interpolated_or_replica_pose(
    poses: &[PresentationPose],
    entity_id: WireEntityId,
    replica_position: [f32; 2],
) -> ([f32; 2], bool) {
    match poses.iter().find(|p| p.entity_id == entity_id) {
        Some(pose) => (pose.position, true),
        None => (replica_position, false),
    }
}

/// Distinct poses for one remote. Consecutive same-position copies are
/// Selective cadence padding and are dropped so Nearby gap-2 still lerps.
///
/// Same pose after a tick gap (idle after global dup-skip) must **not**
/// stretch the current unique tick while a previous different pose is still
/// the lerp A sample — that pulled landing remotes back up toward the last
/// airborne pose each time a grounded Update arrived.
///
/// Stamp the rest pose with the latest hold tick only when a *new* pose
/// starts **and** that rest included a gapped sample (idle-then-move).
/// Consecutive +1 copies are cadence padding and must keep the first unique
/// tick so last_gap stays 2 or 4.
fn unique_entity_poses(
    history: &VecDeque<HistorySample>,
    id: WireEntityId,
) -> Vec<(u64, [f32; 2])> {
    let mut out: Vec<(u64, [f32; 2])> = Vec::with_capacity(history.len());
    let mut last_tick: Option<u64> = None;
    let mut last_pos: Option<[f32; 2]> = None;
    let mut gapped_hold = false;
    for sample in history {
        let Some(pos) = sample.find(id) else {
            continue;
        };
        let tick = sample.server_tick;
        if last_pos == Some(pos) {
            if last_tick.is_some_and(|prev| tick > prev.saturating_add(1)) {
                gapped_hold = true;
            }
            last_tick = Some(tick);
            continue;
        }
        if gapped_hold && let (Some(last), Some(held)) = (out.last_mut(), last_tick) {
            last.0 = held;
        }
        out.push((tick, pos));
        last_tick = Some(tick);
        last_pos = Some(pos);
        gapped_hold = false;
    }
    out
}

fn entity_transform_span(
    history: &VecDeque<HistorySample>,
    id: WireEntityId,
) -> EntityTransformSpan {
    let pts = unique_entity_poses(history, id);
    let mut ticks = [0u64; 8];
    let start = pts.len().saturating_sub(8);
    let suffix = &pts[start..];
    for (i, (tick, _)) in suffix.iter().enumerate() {
        ticks[i] = *tick;
    }
    EntityTransformSpan {
        unique_count: u32::try_from(pts.len()).unwrap_or(u32::MAX),
        oldest_tick: pts.first().map(|(t, _)| *t),
        newest_tick: pts.last().map(|(t, _)| *t),
        last_gap: pts.windows(2).last().map(|w| w[1].0.saturating_sub(w[0].0)),
        ticks,
        ticks_len: u8::try_from(suffix.len()).unwrap_or(8),
    }
}

fn entity_interp_bracket(
    history: &VecDeque<HistorySample>,
    id: WireEntityId,
    render: f64,
) -> EntityInterpBracket {
    let pts = unique_entity_poses(history, id);
    match pts.as_slice() {
        [] => EntityInterpBracket::default(),
        [(tick, _)] => EntityInterpBracket {
            sample_a: Some(*tick),
            sample_b: Some(*tick),
            alpha: 0.0,
            clamped_newest: render >= *tick as f64,
        },
        pts => {
            let oldest = pts[0].0 as f64;
            let newest = pts[pts.len() - 1].0 as f64;
            if render <= oldest {
                return EntityInterpBracket {
                    sample_a: Some(pts[0].0),
                    sample_b: Some(pts[0].0),
                    alpha: 0.0,
                    clamped_newest: false,
                };
            }
            if render >= newest {
                let last = pts[pts.len() - 1].0;
                return EntityInterpBracket {
                    sample_a: Some(last),
                    sample_b: Some(last),
                    alpha: 0.0,
                    clamped_newest: true,
                };
            }
            let mut a_idx = 0usize;
            for (i, (tick, _)) in pts.iter().enumerate() {
                if *tick as f64 <= render {
                    a_idx = i;
                } else {
                    break;
                }
            }
            let b_idx = (a_idx + 1).min(pts.len() - 1);
            if a_idx == b_idx {
                return EntityInterpBracket {
                    sample_a: Some(pts[a_idx].0),
                    sample_b: Some(pts[a_idx].0),
                    alpha: 0.0,
                    clamped_newest: false,
                };
            }
            let tick_a = pts[a_idx].0 as f64;
            let tick_b = pts[b_idx].0 as f64;
            let span = (tick_b - tick_a).max(f64::EPSILON);
            let alpha = ((render - tick_a) / span) as f32;
            EntityInterpBracket {
                sample_a: Some(pts[a_idx].0),
                sample_b: Some(pts[b_idx].0),
                alpha: alpha.clamp(0.0, 1.0),
                clamped_newest: false,
            }
        }
    }
}

fn interp_entity_pose(
    history: &VecDeque<HistorySample>,
    id: WireEntityId,
    render: f64,
    fallback: [f32; 2],
    snaps: &mut u64,
) -> [f32; 2] {
    let pts = unique_entity_poses(history, id);
    match pts.as_slice() {
        [] => fallback,
        [(_, pos)] => *pos,
        pts => {
            if render <= pts[0].0 as f64 {
                return pts[0].1;
            }
            let last = pts.len() - 1;
            if render >= pts[last].0 as f64 {
                return pts[last].1;
            }
            let mut a_idx = 0usize;
            for (i, (tick, _)) in pts.iter().enumerate() {
                if *tick as f64 <= render {
                    a_idx = i;
                } else {
                    break;
                }
            }
            let b_idx = (a_idx + 1).min(last);
            if a_idx == b_idx {
                return pts[a_idx].1;
            }
            let tick_a = pts[a_idx].0 as f64;
            let tick_b = pts[b_idx].0 as f64;
            let span = (tick_b - tick_a).max(f64::EPSILON);
            let alpha = ((render - tick_a) / span) as f32;
            lerp_or_snap(pts[a_idx].1, pts[b_idx].1, alpha, snaps)
        }
    }
}

fn lerp_or_snap(a: [f32; 2], b: [f32; 2], alpha: f32, snaps: &mut u64) -> [f32; 2] {
    if !alpha.is_finite() {
        return a;
    }
    let t = alpha.clamp(0.0, 1.0);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dist = (dx * dx + dy * dy).sqrt();
    if !dist.is_finite() || dist >= INTERPOLATION_SNAP_DISTANCE {
        *snaps = snaps.saturating_add(1);
        return b;
    }
    let x = a[0] + dx * t;
    let y = a[1] + dy * t;
    if x.is_finite() && y.is_finite() {
        [x, y]
    } else {
        a
    }
}

/// Pure lerp helper for unit tests (no teleport snap).
#[cfg(test)]
#[must_use]
fn lerp_position(a: [f32; 2], b: [f32; 2], alpha: f32) -> [f32; 2] {
    if !alpha.is_finite() {
        return a;
    }
    let t = alpha.clamp(0.0, 1.0);
    let x = a[0] + (b[0] - a[0]) * t;
    let y = a[1] + (b[1] - a[1]) * t;
    if x.is_finite() && y.is_finite() {
        [x, y]
    } else {
        a
    }
}

/// Returns (a_idx, b_idx, alpha, held_underrun).
fn find_bracket(history: &VecDeque<HistorySample>, render: f64) -> (usize, usize, f32, bool) {
    let n = history.len();
    debug_assert!(n >= 1);
    let oldest = history[0].server_tick as f64;
    let newest = history[n - 1].server_tick as f64;

    if render <= oldest {
        return (0, 0, 0.0, false);
    }
    if render >= newest {
        return (n - 1, n - 1, 0.0, true);
    }

    let mut a_idx = 0;
    for (i, sample) in history.iter().enumerate() {
        if sample.server_tick as f64 <= render {
            a_idx = i;
        } else {
            break;
        }
    }
    let b_idx = (a_idx + 1).min(n - 1);
    if a_idx == b_idx {
        return (a_idx, b_idx, 0.0, false);
    }
    let tick_a = history[a_idx].server_tick as f64;
    let tick_b = history[b_idx].server_tick as f64;
    let denom = tick_b - tick_a;
    let alpha = if denom.abs() < f64::EPSILON {
        0.0
    } else {
        let raw = (render - tick_a) / denom;
        if raw.is_finite() {
            raw.clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    };
    (a_idx, b_idx, alpha, false)
}

#[cfg(test)]
#[must_use]
fn ticks_to_nanos(ticks: u64) -> u64 {
    ticks.saturating_mul(TICK_DURATION_NANOS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_protocol::{ReplicatedKind, SnapshotEntity};
    use purgatory_simulation::TICK_RATE_HZ;
    use std::time::Duration;

    fn eid(index: u32, generation: u32) -> WireEntityId {
        WireEntityId { index, generation }
    }

    fn entity(index: u32, generation: u32, x: f32, y: f32) -> SnapshotEntity {
        SnapshotEntity {
            entity_id: eid(index, generation),
            kind: ReplicatedKind::Player,
            position: [x, y],
            velocity: [0.0, 0.0],
        }
    }

    fn snap(
        seq: u32,
        tick: u64,
        local: WireEntityId,
        entities: Vec<SnapshotEntity>,
    ) -> WorldSnapshot {
        WorldSnapshot::from_poses(seq, tick, local, entities)
    }

    #[test]
    fn midpoint_and_alpha_extremes() {
        let a = [0.0, 0.0];
        let b = [10.0, 20.0];
        assert_eq!(lerp_position(a, b, 0.0), a);
        assert_eq!(lerp_position(a, b, 1.0), b);
        let mid = lerp_position(a, b, 0.5);
        assert!((mid[0] - 5.0).abs() < 1e-5);
        assert!((mid[1] - 10.0).abs() < 1e-5);
    }

    #[test]
    fn alpha_math_uses_tick_duration_not_literal_thirty() {
        assert_eq!(TICK_RATE_HZ, 30);
        let delay_nanos = ticks_to_nanos(INTERPOLATION_DELAY_TICKS);
        assert_eq!(delay_nanos, INTERPOLATION_DELAY_TICKS * TICK_DURATION_NANOS);
        let tick_a = 10.0_f64;
        let tick_b = 12.0_f64;
        let render = 11.0;
        let alpha = ((render - tick_a) / (tick_b - tick_a)) as f32;
        assert!((alpha - 0.5).abs() < 1e-5);
        let pos = lerp_position([0.0, 0.0], [4.0, 0.0], alpha);
        assert!((pos[0] - 2.0).abs() < 1e-5);
        assert!(pos[0].is_finite() && pos[1].is_finite());
    }

    #[test]
    fn history_sample_drops_interactables() {
        let local = eid(1, 1);
        let sample = HistorySample::from_snapshot(&snap(
            1,
            1,
            local,
            vec![
                entity(1, 1, 0.0, 0.0),
                SnapshotEntity {
                    entity_id: eid(9, 1),
                    kind: ReplicatedKind::Interactable,
                    position: [5.0, 1.0],
                    velocity: [0.0, 0.0],
                },
                entity(2, 1, 3.0, 0.0),
            ],
        ));
        assert_eq!(sample.entities.len(), 2);
        assert!(sample.entities.iter().all(|e| e.entity_id != eid(9, 1)));
    }

    #[test]
    fn non_finite_alpha_holds_a() {
        assert_eq!(lerp_position([1.0, 2.0], [9.0, 9.0], f32::NAN), [1.0, 2.0]);
        let mut snaps = 0;
        let out = lerp_or_snap([1.0, 2.0], [3.0, 2.0], f32::NAN, &mut snaps);
        assert_eq!(out, [1.0, 2.0]);
    }

    #[test]
    fn history_bounded_and_ordered() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let t0 = Instant::now();
        for i in 0..(INTERPOLATION_HISTORY_CAP + 5) {
            buf.push(&snap(
                i as u32 + 1,
                i as u64,
                local,
                vec![entity(2, 1, i as f32, 0.0)],
            ));
        }
        assert_eq!(buf.depth(), INTERPOLATION_HISTORY_CAP);
        let ticks: Vec<_> = buf.history.iter().map(|h| h.server_tick).collect();
        for w in ticks.windows(2) {
            assert!(w[0] <= w[1]);
        }
        let _ = t0;
    }

    #[test]
    fn clear_drops_history_and_clock() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        buf.push(&snap(1, 100, local, vec![entity(2, 1, 1.0, 0.0)]));
        buf.sample(Instant::now());
        buf.clear();
        assert_eq!(buf.depth(), 0);
        assert_eq!(buf.estimated_server_tick(), 0.0);
        assert!(buf.poses().is_empty());
    }

    #[test]
    fn local_entity_not_in_poses_remote_is() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(
            1,
            10,
            local,
            vec![entity(1, 1, 0.0, 0.0), entity(2, 1, 5.0, 0.0)],
        ));
        buf.push(&snap(
            2,
            13,
            local,
            vec![entity(1, 1, 0.0, 0.0), entity(2, 1, 8.0, 0.0)],
        ));
        // Force render between 10 and 13.
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.5;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        assert!(buf.poses().iter().all(|p| p.entity_id != local));
        assert!(buf.poses().iter().any(|p| p.entity_id == remote));
    }

    #[test]
    fn interpolate_between_ticks_10_and_12() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(10, 10, local, vec![entity(2, 1, 0.0, 0.0)]));
        buf.push(&snap(12, 12, local, vec![entity(2, 1, 4.0, 0.0)]));
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!(
            (pose.position[0] - 2.0).abs() < 1e-3,
            "got {}",
            pose.position[0]
        );
    }

    #[test]
    fn spawn_only_in_b_hidden_until_render_reaches_b() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![]));
        buf.push(&snap(2, 12, local, vec![entity(2, 1, 7.0, 3.0)]));
        // render between A and B
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 0.5;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        assert!(
            buf.poses().iter().all(|p| p.entity_id != remote),
            "spawn must wait until render reaches B"
        );
        // render at/past B
        buf.estimated_server_tick = 12.0 + INTERPOLATION_DELAY_TICKS as f64;
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert_eq!(pose.position, [7.0, 3.0]);
        assert!((pose.position[0] - 0.0).abs() > 1.0, "not origin");
    }

    #[test]
    fn despawn_waits_for_render_timeline_not_wire_arrival() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![entity(2, 1, 4.0, 0.0)]));
        buf.push(&snap(2, 12, local, vec![]));
        // render still before B: entity must remain at A
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 0.5;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert_eq!(pose.position, [4.0, 0.0]);
        // after render passes B: gone
        buf.estimated_server_tick = 12.0 + INTERPOLATION_DELAY_TICKS as f64;
        buf.last_render_tick = 0.0;
        buf.sample(now);
        assert!(buf.poses().iter().all(|p| p.entity_id != remote));
    }

    #[test]
    fn clock_monotonic_under_late_arrival() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let t0 = Instant::now();
        buf.push(&snap(1, 100, local, vec![]));
        buf.sample(t0);
        let est1 = buf.estimated_server_tick();
        let rend1 = buf.last_render_tick();
        // Wall time advances.
        let t1 = t0 + Duration::from_millis(50);
        buf.sample(t1);
        let est2 = buf.estimated_server_tick();
        let rend2 = buf.last_render_tick();
        assert!(est2 >= est1);
        assert!(rend2 >= rend1);
        // Late snapshot with older server_tick must not rewind.
        buf.push(&snap(2, 50, local, vec![]));
        assert!(buf.estimated_server_tick() >= est2);
        let t2 = t1 + Duration::from_millis(10);
        buf.sample(t2);
        assert!(buf.estimated_server_tick() >= est2);
        assert!(buf.last_render_tick() >= rend2);
    }

    #[test]
    fn generation_reuse_does_not_cross_lerp() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let g1 = eid(5, 1);
        let g2 = eid(5, 2);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![entity(5, 1, 0.0, 0.0)]));
        buf.push(&snap(2, 12, local, vec![entity(5, 2, 100.0, 0.0)]));
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        // Between A and B: still g1 at A position (g2 not yet visible).
        assert!(buf.poses().iter().any(|p| p.entity_id == g1));
        assert!(buf.poses().iter().all(|p| p.entity_id != g2));
        let p = buf.poses().iter().find(|p| p.entity_id == g1).unwrap();
        assert!((p.position[0] - 0.0).abs() < 1e-3);
        // Past B: only g2 at B, not lerped from g1.
        buf.estimated_server_tick = 12.0 + INTERPOLATION_DELAY_TICKS as f64;
        buf.last_render_tick = 0.0;
        buf.sample(now);
        assert!(buf.poses().iter().all(|p| p.entity_id != g1));
        let p2 = buf.poses().iter().find(|p| p.entity_id == g2).unwrap();
        assert_eq!(p2.position, [100.0, 0.0]);
    }

    #[test]
    fn underrun_holds_newest_and_counts() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![entity(2, 1, 3.0, 1.0)]));
        buf.push(&snap(2, 11, local, vec![entity(2, 1, 4.0, 1.0)]));
        buf.estimated_server_tick = 100.0 + INTERPOLATION_DELAY_TICKS as f64;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        let before = buf.holds;
        buf.sample(now);
        assert!(buf.holds > before);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert_eq!(pose.position, [4.0, 1.0]);
    }

    #[test]
    fn teleport_snaps_instead_of_long_lerp() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![entity(2, 1, 0.0, 0.0)]));
        buf.push(&snap(
            2,
            12,
            local,
            vec![entity(2, 1, INTERPOLATION_SNAP_DISTANCE + 1.0, 0.0)],
        ));
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        let before = buf.snaps;
        buf.sample(now);
        assert!(buf.snaps > before);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!((pose.position[0] - (INTERPOLATION_SNAP_DISTANCE + 1.0)).abs() < 1e-3);
    }

    #[test]
    fn two_remotes_opposite_motion_interpolates() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let a = eid(2, 1);
        let b = eid(3, 1);
        let now = Instant::now();
        buf.push(&snap(
            1,
            10,
            local,
            vec![entity(2, 1, 0.0, 0.0), entity(3, 1, 4.0, 0.0)],
        ));
        buf.push(&snap(
            2,
            12,
            local,
            vec![entity(2, 1, 4.0, 0.0), entity(3, 1, 0.0, 0.0)],
        ));
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pa = buf.poses().iter().find(|p| p.entity_id == a).unwrap();
        let pb = buf.poses().iter().find(|p| p.entity_id == b).unwrap();
        assert!((pa.position[0] - 2.0).abs() < 1e-2);
        assert!((pb.position[0] - 2.0).abs() < 1e-2);
    }

    #[test]
    fn impairment_uneven_arrival_skipped_tick_stays_bounded() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let t0 = Instant::now();
        buf.push(&snap(10, 10, local, vec![entity(2, 1, 0.0, 0.0)]));
        // Delayed arrival of tick 11 (late wall time), then skip to 13.
        let t_late = t0 + Duration::from_millis(80);
        buf.push(&snap(11, 11, local, vec![entity(2, 1, 2.0, 0.0)]));
        buf.sample(t_late);
        let est_after_late = buf.estimated_server_tick();
        let rend_after_late = buf.last_render_tick();
        let t_skip = t_late + Duration::from_millis(5);
        buf.push(&snap(13, 13, local, vec![entity(2, 1, 6.0, 0.0)]));
        buf.sample(t_skip);
        assert!(buf.estimated_server_tick() >= est_after_late);
        assert!(buf.last_render_tick() >= rend_after_late);
        if let Some(pose) = buf.poses().iter().find(|p| p.entity_id == remote) {
            assert!(pose.position[0] >= 0.0 && pose.position[0] <= 6.0);
            assert!(pose.position[0].is_finite());
        }
    }

    #[test]
    fn snap_distance_is_presentation_constant() {
        const { assert!(INTERPOLATION_SNAP_DISTANCE > 0.0) };
        const { assert!(INTERPOLATION_DELAY_TICKS >= 2) };
        const { assert!(INTERPOLATION_HISTORY_CAP == 16) };
    }

    #[test]
    fn consecutive_stale_remote_tick_is_skipped_and_lerp_spans_gap() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(
            1,
            10,
            local,
            vec![entity(1, 1, 0.0, 0.0), entity(2, 1, 0.0, 0.0)],
        ));
        buf.push(&snap(
            2,
            11,
            local,
            vec![entity(1, 1, 5.0, 0.0), entity(2, 1, 0.0, 0.0)],
        ));
        buf.push(&snap(
            3,
            12,
            local,
            vec![entity(1, 1, 10.0, 0.0), entity(2, 1, 4.0, 0.0)],
        ));
        assert_eq!(buf.depth(), 2);
        assert_eq!(buf.dup_skips, 1);
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!((pose.position[0] - 2.0).abs() < 1e-2);
    }

    #[test]
    fn idle_then_move_keeps_latest_hold_tick() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![entity(2, 1, 0.0, 0.0)]));
        buf.push(&snap(2, 11, local, vec![entity(2, 1, 0.0, 0.0)]));
        buf.push(&snap(3, 12, local, vec![entity(2, 1, 0.0, 0.0)]));
        buf.push(&snap(4, 13, local, vec![entity(2, 1, 4.0, 0.0)]));
        assert_eq!(buf.depth(), 3);
        assert_eq!(buf.dup_skips, 1);
        buf.estimated_server_tick = 12.0 + INTERPOLATION_DELAY_TICKS as f64 + 0.5;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!((pose.position[0] - 2.0).abs() < 1e-2);
    }

    #[test]
    fn interpolated_or_replica_pose_prefers_interp() {
        let id = eid(2, 1);
        let poses = [PresentationPose {
            entity_id: id,
            position: [1.5, 0.25],
        }];
        let (pos, used) = interpolated_or_replica_pose(&poses, id, [9.0, 9.0]);
        assert!(used);
        assert_eq!(pos, [1.5, 0.25]);
        let missing = eid(3, 1);
        let (fallback, used_fb) = interpolated_or_replica_pose(&poses, missing, [9.0, 8.0]);
        assert!(!used_fb);
        assert_eq!(fallback, [9.0, 8.0]);
    }

    #[test]
    fn hitch_catch_up_does_not_hold_newest_after_burst() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let t0 = Instant::now();
        buf.push_at(&snap(1, 10, local, vec![entity(2, 1, 0.0, 0.0)]), t0);
        buf.sample(t0);
        let hitch = t0 + Duration::from_millis(200);
        for i in 1..=6u32 {
            let tick = 10 + u64::from(i);
            buf.push_at(
                &snap(
                    i + 1,
                    tick,
                    local,
                    vec![entity(2, 1, tick as f32 * 0.4, 0.0)],
                ),
                hitch,
            );
        }
        buf.sample(hitch);
        let newest = 16.0_f64;
        let auth = (newest as f32) * 0.4;
        assert!(
            buf.estimated_server_tick() < newest + 1.0,
            "estimated {} must not include hitch wall time on top of snapshot catch-up",
            buf.estimated_server_tick()
        );
        let diag = buf.diagnostics();
        assert!(
            !diag.clamped_newest,
            "render must stay behind newest after catch-up"
        );
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!(
            (pose.position[0] - auth).abs() > 0.2,
            "interp {} must lag auth {auth}",
            pose.position[0]
        );
    }

    #[test]
    fn steady_30hz_push_60hz_sample_interp_lags_auth() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let mut t = Instant::now();
        let mut equal = 0u32;
        let mut compared = 0u32;
        for tick in 0..60u64 {
            let x = tick as f32 * 0.5;
            buf.push_at(
                &snap(
                    u32::try_from(tick).unwrap() + 1,
                    tick,
                    local,
                    vec![entity(1, 1, 0.0, 0.0), entity(2, 1, x, 0.0)],
                ),
                t,
            );
            buf.sample(t);
            t += TICK_DURATION / 2;
            buf.sample(t);
            t += TICK_DURATION / 2;
            if tick < 10 {
                continue;
            }
            compared += 1;
            let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
            if (pose.position[0] - x).abs() < 1e-3 {
                equal += 1;
            }
        }
        assert!(compared > 0);
        assert!(
            equal * 4 < compared,
            "interp matched auth on {equal}/{compared} frames (delay is not applied)"
        );
        let diag = buf.diagnostics();
        assert!(diag.history_depth >= 2);
        assert!(!diag.clamped_newest);
        assert!(diag.render_tick + 1.0 < diag.newest_tick.unwrap() as f64);
    }

    #[test]
    fn leave_then_reenter_does_not_reseed_history() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        buf.push(&snap(1, 10, local, vec![entity(2, 1, 0.0, 0.0)]));
        buf.push(&snap(2, 12, local, vec![entity(2, 1, 4.0, 0.0)]));
        let reseeds = buf.diagnostics().reseeds;
        buf.push(&snap(3, 14, local, vec![]));
        buf.push(&snap(4, 16, local, vec![entity(2, 1, 20.0, 0.0)]));
        assert_eq!(buf.diagnostics().reseeds, reseeds);
        assert_eq!(buf.depth(), 4);
        let (count, oldest, newest) = buf.remote_history_span(remote);
        assert_eq!(count, 3);
        assert_eq!(oldest, Some(10));
        assert_eq!(newest, Some(16));
        buf.estimated_server_tick = 14.0 + INTERPOLATION_DELAY_TICKS as f64;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        assert!(
            buf.poses().iter().all(|p| p.entity_id != remote),
            "despawn waits until render reaches the empty sample B"
        );
        buf.estimated_server_tick = 16.0 + INTERPOLATION_DELAY_TICKS as f64;
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert_eq!(pose.position, [20.0, 0.0]);
    }

    fn npc(index: u32, generation: u32, x: f32, y: f32) -> SnapshotEntity {
        SnapshotEntity {
            entity_id: eid(index, generation),
            kind: ReplicatedKind::Npc,
            position: [x, y],
            velocity: [0.0, 0.0],
        }
    }

    #[test]
    fn padded_cadence_two_lerps_across_unique_poses() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        for tick in 10..=16u64 {
            let rx = ((tick - 10) / 2) as f32 * 2.0;
            buf.push_at(
                &snap(
                    u32::try_from(tick).unwrap(),
                    tick,
                    local,
                    vec![
                        entity(1, 1, 0.0, 0.0),
                        entity(2, 1, rx, 0.0),
                        npc(3, 1, tick as f32, 0.0),
                    ],
                ),
                now,
            );
        }
        buf.estimated_server_tick = 10.0 + INTERPOLATION_DELAY_TICKS as f64 + 1.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 0.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!(
            (pose.position[0] - 1.0).abs() < 1e-3,
            "cadence-2 padding from other remotes must not hold-then-jump, got {}",
            pose.position[0]
        );
        let br = buf.remote_entity_bracket(remote);
        assert!(!br.clamped_newest);
        assert_eq!(br.sample_a, Some(10));
        assert_eq!(br.sample_b, Some(12));
        let span = buf.remote_unique_transform_span(remote);
        assert_eq!(span.last_gap, Some(2));
        assert!(span.unique_count >= 3);
        assert_eq!(span.oldest_tick, Some(10));
        assert_eq!(span.newest_tick, Some(16));
    }

    #[test]
    fn delay_three_does_not_clamp_entity_on_unique_cadence_two() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let t0 = Instant::now();
        let mut clamped = 0u32;
        for tick in 10..=40u64 {
            let rx = ((tick - 10) / 2) as f32 * 0.4;
            buf.push_at(
                &snap(
                    u32::try_from(tick).unwrap(),
                    tick,
                    local,
                    vec![
                        entity(1, 1, 0.0, 0.0),
                        entity(2, 1, rx, 0.0),
                        npc(3, 1, tick as f32 * 0.1, 0.0),
                    ],
                ),
                t0 + TICK_DURATION * tick as u32,
            );
            buf.sample(t0 + TICK_DURATION * tick as u32);
            buf.sample(t0 + TICK_DURATION * tick as u32 + TICK_DURATION / 2);
            if tick < 16 {
                continue;
            }
            if buf.remote_entity_bracket(remote).clamped_newest {
                clamped = clamped.saturating_add(1);
            }
        }
        assert_eq!(clamped, 0, "3-tick delay must cover delivered Nearby gap 2");
        assert_eq!(buf.remote_unique_transform_span(remote).last_gap, Some(2));
    }

    #[test]
    fn delay_three_clamps_entity_on_unique_cadence_four() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let t0 = Instant::now();
        let mut clamped = 0u32;
        for tick in 10..=40u64 {
            let rx = ((tick - 10) / 4) as f32 * 0.8;
            buf.push_at(
                &snap(
                    u32::try_from(tick).unwrap(),
                    tick,
                    local,
                    vec![
                        entity(1, 1, 0.0, 0.0),
                        entity(2, 1, rx, 0.0),
                        npc(3, 1, tick as f32 * 0.1, 0.0),
                    ],
                ),
                t0 + TICK_DURATION * tick as u32,
            );
            buf.sample(t0 + TICK_DURATION * tick as u32);
            buf.sample(t0 + TICK_DURATION * tick as u32 + TICK_DURATION / 2);
            if tick < 20 {
                continue;
            }
            if buf.remote_entity_bracket(remote).clamped_newest {
                clamped = clamped.saturating_add(1);
            }
        }
        assert!(
            clamped > 0,
            "3-tick delay is structurally too small for delivered gap 4"
        );
        assert_eq!(buf.remote_unique_transform_span(remote).last_gap, Some(4));
    }

    #[test]
    fn landing_ground_hold_does_not_raise_interp_y() {
        let mut buf = InterpolationBuffer::new();
        let local = eid(1, 1);
        let remote = eid(2, 1);
        let now = Instant::now();
        let fall = [(10u64, 2.0f32), (12, 1.0), (14, 0.0)];
        for (tick, y) in fall {
            buf.push_at(
                &snap(
                    u32::try_from(tick).unwrap(),
                    tick,
                    local,
                    vec![
                        entity(1, 1, 0.0, 0.0),
                        entity(2, 1, 0.0, y),
                        npc(3, 1, tick as f32, 0.0),
                    ],
                ),
                now,
            );
        }
        for tick in [16u64, 18, 20, 22] {
            buf.push_at(
                &snap(
                    u32::try_from(tick).unwrap(),
                    tick,
                    local,
                    vec![
                        entity(1, 1, 0.0, 0.0),
                        entity(2, 1, 0.0, 0.0),
                        npc(3, 1, tick as f32, 0.0),
                    ],
                ),
                now,
            );
        }
        buf.estimated_server_tick = 16.0;
        buf.last_advance_at = Some(now);
        buf.last_render_tick = 13.0;
        buf.sample(now);
        let pose = buf.poses().iter().find(|p| p.entity_id == remote).unwrap();
        assert!(
            pose.position[1] <= 0.51,
            "extending grounded unique tick must not pull Y back toward airborne A, got {}",
            pose.position[1]
        );
        let br = buf.remote_entity_bracket(remote);
        assert_eq!(br.sample_a, Some(12));
        assert_eq!(br.sample_b, Some(14));
        let span = buf.remote_unique_transform_span(remote);
        assert_eq!(span.newest_tick, Some(14));
    }
}
