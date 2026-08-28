//! Client-only remote entity interpolation (Phase 5.3).
//!
//! Presentation layer separate from [`crate::replica::ReplicatedWorld`].
//! Does not modify authoritative replica state, input, or the server.

use std::collections::VecDeque;
use std::time::Instant;

use purgatory_protocol::{WireEntityId, WorldSnapshot};
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
        }
    }

    /// Push a replica-accepted authoritative snapshot. Late arrivals never
    /// rewind the estimated clock (forward catch-up only).
    pub fn push(&mut self, snap: &WorldSnapshot) {
        let tick = snap.server_tick as f64;
        if self.last_advance_at.is_none() {
            self.estimated_server_tick = tick;
            self.last_advance_at = Some(Instant::now());
            self.last_render_tick = (tick - INTERPOLATION_DELAY_TICKS as f64).max(0.0);
        } else if tick > self.estimated_server_tick {
            self.estimated_server_tick = tick;
        }

        if self
            .history
            .back()
            .is_some_and(|h| h.server_tick > snap.server_tick)
        {
            // Out-of-order by tick: still store if newer sequence was accepted
            // by replica; keep history sorted by server_tick.
            let sample = HistorySample::from_snapshot(snap);
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
                *back = HistorySample::from_snapshot(snap);
            }
        } else {
            self.history.push_back(HistorySample::from_snapshot(snap));
        }

        while self.history.len() > INTERPOLATION_HISTORY_CAP {
            self.history.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.history.clear();
        self.estimated_server_tick = 0.0;
        self.last_render_tick = 0.0;
        self.last_advance_at = None;
        self.poses.clear();
        self.holds = 0;
        self.snaps = 0;
        self.last_alpha = 0.0;
        self.last_bracket_a = None;
        self.last_bracket_b = None;
        self.diag = InterpDiagnostics {
            enabled: true,
            delay_ticks: INTERPOLATION_DELAY_TICKS,
            delay_ms: interpolation_delay_ms(),
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
                let pos = if a_idx == b_idx {
                    entity.position
                } else if let Some(pos_a) = a.find(entity.entity_id) {
                    lerp_or_snap(pos_a, entity.position, alpha, &mut self.snaps)
                } else {
                    // Spawn in B only: show at B once render reached B.
                    entity.position
                };
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
                let pos = if let Some(pos_b) = b.find(entity.entity_id) {
                    lerp_or_snap(entity.position, pos_b, alpha, &mut self.snaps)
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
}
