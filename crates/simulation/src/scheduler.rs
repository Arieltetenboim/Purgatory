//! Simulation-time scheduler. Gameplay timers use [`SimulationTick`], not wall clocks.
//!
//! Critical due work is intended to complete in the required tick. A hard
//! ceiling is a pathological-overload safeguard: hitting it delays leftover
//! critical work and is a visible invariant failure, not a normal budget.
//! Deferred work is budgeted FIFO with a progress guarantee.

use std::collections::HashMap;
use std::fmt;

use crate::entity::EntityId;
use crate::time::SimulationTick;

/// Live scheduler slots. Rejects new work rather than growing unbounded.
pub const SCHEDULER_CAPACITY: u32 = 4096;

/// Pathological-overload ceiling for the Critical lane in one drain pass.
///
/// Normal correctness-critical work must complete this tick. Exceeding this
/// ceiling means overload: remainder carries forward and
/// [`Scheduler::critical_ceiling_hits`] increments. That delay can change
/// gameplay timing; it is a safeguard against an unlimited drain, not a
/// policy that critical work is deferrable.
pub const CRITICAL_DRAIN_CEILING: u32 = 1024;

/// Deferred jobs executed per tick. At least one due job runs while any remain.
pub const DEFERRED_DRAIN_BUDGET: u32 = 32;

/// Generational timer identity. A cancelled or expired handle cannot address
/// a reused slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TimerId {
    index: u32,
    generation: u32,
}

impl TimerId {
    #[must_use]
    pub const fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for TimerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}

/// Who owns scheduled work. Owner disappearance cancels owned jobs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ScheduleOwner {
    World,
    Entity(EntityId),
}

/// Critical work is correctness-timed. Deferred work may spread across ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum WorkLane {
    Critical,
    Deferred,
}

/// Closed 6F job kinds. Future gameplay adds variants rather than a second timer engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledKind {
    ExpireEffect(crate::effect::EffectId),
    CompleteAction(crate::action::ActionId),
    /// Phase 9A: advance ability Windup → Active → Recovery on the Action slot.
    AdvanceAction(crate::action::ActionId),
    SpawnDue(crate::spawn_schedule::SpawnRequestId),
    DespawnEntity(EntityId),
    /// Phase 7.2: apply one Pulse damage tick and reschedule or expire.
    PulseTick(crate::effect::EffectId),
    TestProbe {
        token: u32,
    },
    RaiseEvent {
        token: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Job {
    generation: u32,
    due: SimulationTick,
    seq: u64,
    owner: ScheduleOwner,
    lane: WorkLane,
    kind: ScheduledKind,
}

/// A job that passed liveness checks and is ready to apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiredJob {
    pub id: TimerId,
    pub owner: ScheduleOwner,
    pub kind: ScheduledKind,
    pub due: SimulationTick,
}

/// Result of one drain pass.
#[derive(Clone, Debug, Default)]
pub struct DrainOutcome {
    pub fired: Vec<FiredJob>,
    pub remaining_due: u32,
    /// Critical lane hit [`CRITICAL_DRAIN_CEILING`] with due work left.
    pub ceiling_hit: bool,
    /// Deferred lane stopped at [`DEFERRED_DRAIN_BUDGET`] with due work left.
    pub budget_exhausted: bool,
}

/// Authoritative tick scheduler. Not a cron calendar. Not wall-clock.
pub struct Scheduler {
    slots: Vec<Option<Job>>,
    generations: Vec<u32>,
    free: Vec<u32>,
    by_owner: HashMap<EntityId, Vec<TimerId>>,
    next_seq: u64,
    live: u32,
    critical_ceiling_hits: u64,
    deferred_exhausted: u64,
    scheduled_total: u64,
    cancelled_total: u64,
    critical_executed_total: u64,
    deferred_executed_total: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            generations: Vec::new(),
            free: Vec::new(),
            by_owner: HashMap::new(),
            next_seq: 0,
            live: 0,
            critical_ceiling_hits: 0,
            deferred_exhausted: 0,
            scheduled_total: 0,
            cancelled_total: 0,
            critical_executed_total: 0,
            deferred_executed_total: 0,
        }
    }

    #[must_use]
    pub fn live_count(&self) -> u32 {
        self.live
    }

    #[must_use]
    pub fn critical_ceiling_hits(&self) -> u64 {
        self.critical_ceiling_hits
    }

    #[must_use]
    pub fn deferred_exhausted(&self) -> u64 {
        self.deferred_exhausted
    }

    #[must_use]
    pub fn scheduled_total(&self) -> u64 {
        self.scheduled_total
    }

    #[must_use]
    pub fn cancelled_total(&self) -> u64 {
        self.cancelled_total
    }

    #[must_use]
    pub fn critical_executed_total(&self) -> u64 {
        self.critical_executed_total
    }

    #[must_use]
    pub fn deferred_executed_total(&self) -> u64 {
        self.deferred_executed_total
    }

    #[must_use]
    pub fn get(&self, id: TimerId) -> Option<ScheduledKind> {
        let job = self.slot(id)?;
        Some(job.kind)
    }

    /// Schedule at an absolute simulation tick.
    pub fn schedule_at(
        &mut self,
        due: SimulationTick,
        owner: ScheduleOwner,
        lane: WorkLane,
        kind: ScheduledKind,
    ) -> Option<TimerId> {
        if self.live >= SCHEDULER_CAPACITY {
            return None;
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        let (index, generation) = self.allocate_slot();
        let job = Job {
            generation,
            due,
            seq,
            owner,
            lane,
            kind,
        };
        self.slots[index as usize] = Some(job);
        self.live = self.live.saturating_add(1);
        self.scheduled_total = self.scheduled_total.saturating_add(1);
        if let ScheduleOwner::Entity(entity) = owner {
            self.by_owner
                .entry(entity)
                .or_default()
                .push(TimerId { index, generation });
        }
        Some(TimerId { index, generation })
    }

    /// Schedule `ticks` after `now`. `0` means due at `now` (next eligible drain).
    pub fn schedule_after(
        &mut self,
        now: SimulationTick,
        ticks: u64,
        owner: ScheduleOwner,
        lane: WorkLane,
        kind: ScheduledKind,
    ) -> Option<TimerId> {
        self.schedule_at(now.saturating_add_ticks(ticks), owner, lane, kind)
    }

    /// Cancel a live timer. Expired/stale ids are a controlled no-op.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        let Some(job) = self.take_slot(id) else {
            return false;
        };
        self.unlink_owner(job.owner, id);
        self.cancelled_total = self.cancelled_total.saturating_add(1);
        true
    }

    /// Cancel every job owned by `entity`. Missing owner is a no-op.
    pub fn cancel_owner(&mut self, entity: EntityId) -> u32 {
        let Some(ids) = self.by_owner.remove(&entity) else {
            return 0;
        };
        let mut n = 0u32;
        for id in ids {
            if self.take_slot(id).is_some() {
                n = n.saturating_add(1);
            }
        }
        self.cancelled_total = self.cancelled_total.saturating_add(u64::from(n));
        n
    }

    /// Count live jobs due on or before `now` in `lane`.
    #[must_use]
    pub fn due_count(&self, now: SimulationTick, lane: WorkLane) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|job| job.lane == lane && job.due <= now)
            .count() as u32
    }

    /// Drain due work for one lane.
    ///
    /// Collects the eligible set first, then fires. Jobs scheduled during
    /// application of this outcome are not included (no same-pass recursion).
    pub fn drain_due(&mut self, now: SimulationTick, lane: WorkLane) -> DrainOutcome {
        let limit = match lane {
            WorkLane::Critical => CRITICAL_DRAIN_CEILING,
            WorkLane::Deferred => DEFERRED_DRAIN_BUDGET.max(1),
        };
        let mut eligible: Vec<(u64, u64, u32, u32)> = Vec::new();
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(job) = slot else {
                continue;
            };
            if job.lane != lane || job.due > now {
                continue;
            }
            eligible.push((job.due.get(), job.seq, index as u32, job.generation));
        }
        eligible.sort_unstable();
        let remaining_after_full = eligible.len().saturating_sub(limit as usize) as u32;
        let take = (limit as usize).min(eligible.len());
        let mut fired = Vec::with_capacity(take);
        for &(_, _, index, generation) in eligible.iter().take(take) {
            let id = TimerId { index, generation };
            let Some(job) = self.take_slot(id) else {
                continue;
            };
            self.unlink_owner(job.owner, id);
            fired.push(FiredJob {
                id,
                owner: job.owner,
                kind: job.kind,
                due: job.due,
            });
        }
        let executed = u64::try_from(fired.len()).unwrap_or(u64::MAX);
        match lane {
            WorkLane::Critical => {
                self.critical_executed_total =
                    self.critical_executed_total.saturating_add(executed);
            }
            WorkLane::Deferred => {
                self.deferred_executed_total =
                    self.deferred_executed_total.saturating_add(executed);
            }
        }
        let remaining_due = remaining_after_full;
        let mut ceiling_hit = false;
        let mut budget_exhausted = false;
        match lane {
            WorkLane::Critical if remaining_due > 0 => {
                ceiling_hit = true;
                self.critical_ceiling_hits = self.critical_ceiling_hits.saturating_add(1);
            }
            WorkLane::Deferred if remaining_due > 0 => {
                budget_exhausted = true;
                self.deferred_exhausted = self.deferred_exhausted.saturating_add(1);
            }
            _ => {}
        }
        DrainOutcome {
            fired,
            remaining_due,
            ceiling_hit,
            budget_exhausted,
        }
    }

    fn allocate_slot(&mut self) -> (u32, u32) {
        if let Some(index) = self.free.pop() {
            let generation = self.generations[index as usize];
            (index, generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("scheduler slot index");
            self.slots.push(None);
            self.generations.push(1);
            (index, 1)
        }
    }

    fn slot(&self, id: TimerId) -> Option<&Job> {
        let job = self.slots.get(id.index as usize)?.as_ref()?;
        (job.generation == id.generation).then_some(job)
    }

    fn take_slot(&mut self, id: TimerId) -> Option<Job> {
        let slot = self.slots.get_mut(id.index as usize)?;
        let job = slot.take()?;
        if job.generation != id.generation {
            *slot = Some(job);
            return None;
        }
        let next = next_generation(job.generation);
        self.generations[id.index as usize] = next;
        self.free.push(id.index);
        self.live = self.live.saturating_sub(1);
        Some(job)
    }

    fn unlink_owner(&mut self, owner: ScheduleOwner, id: TimerId) {
        let ScheduleOwner::Entity(entity) = owner else {
            return;
        };
        let Some(list) = self.by_owner.get_mut(&entity) else {
            return;
        };
        list.retain(|&existing| existing != id);
        if list.is_empty() {
            self.by_owner.remove(&entity);
        }
    }
}

fn next_generation(current: u32) -> u32 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(token: u32) -> ScheduledKind {
        ScheduledKind::TestProbe { token }
    }

    #[test]
    fn future_tick_does_not_fire_early() {
        let mut s = Scheduler::new();
        let now = SimulationTick::from_count(10);
        let id = s
            .schedule_at(
                SimulationTick::from_count(12),
                ScheduleOwner::World,
                WorkLane::Critical,
                probe(1),
            )
            .unwrap();
        let drain = s.drain_due(now, WorkLane::Critical);
        assert!(drain.fired.is_empty());
        assert!(s.get(id).is_some());
        let later = s.drain_due(SimulationTick::from_count(12), WorkLane::Critical);
        assert_eq!(later.fired.len(), 1);
        assert_eq!(later.fired[0].id, id);
        assert!(s.get(id).is_none());
    }

    #[test]
    fn same_tick_order_follows_schedule_sequence() {
        let mut s = Scheduler::new();
        let due = SimulationTick::from_count(5);
        let a = s
            .schedule_at(due, ScheduleOwner::World, WorkLane::Critical, probe(1))
            .unwrap();
        let b = s
            .schedule_at(due, ScheduleOwner::World, WorkLane::Critical, probe(2))
            .unwrap();
        let drain = s.drain_due(due, WorkLane::Critical);
        assert_eq!(drain.fired.len(), 2);
        assert_eq!(drain.fired[0].id, a);
        assert_eq!(drain.fired[1].id, b);
    }

    #[test]
    fn cancelled_job_never_fires() {
        let mut s = Scheduler::new();
        let due = SimulationTick::from_count(3);
        let id = s
            .schedule_at(due, ScheduleOwner::World, WorkLane::Critical, probe(7))
            .unwrap();
        assert!(s.cancel(id));
        assert!(!s.cancel(id));
        let drain = s.drain_due(due, WorkLane::Critical);
        assert!(drain.fired.is_empty());
        assert!(s.get(id).is_none());
    }

    #[test]
    fn stale_timer_id_does_not_address_reuse() {
        let mut s = Scheduler::new();
        let due = SimulationTick::from_count(1);
        let first = s
            .schedule_at(due, ScheduleOwner::World, WorkLane::Critical, probe(1))
            .unwrap();
        assert!(s.cancel(first));
        let second = s
            .schedule_at(due, ScheduleOwner::World, WorkLane::Critical, probe(2))
            .unwrap();
        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert!(!s.cancel(first));
        let drain = s.drain_due(due, WorkLane::Critical);
        assert_eq!(drain.fired.len(), 1);
        assert_eq!(drain.fired[0].id, second);
    }

    #[test]
    fn owner_cancel_drops_owned_jobs() {
        let mut s = Scheduler::new();
        let entity = EntityId::from_raw(4, 1);
        let due = SimulationTick::from_count(8);
        let owned = s
            .schedule_at(
                due,
                ScheduleOwner::Entity(entity),
                WorkLane::Critical,
                probe(1),
            )
            .unwrap();
        let world_job = s
            .schedule_at(due, ScheduleOwner::World, WorkLane::Critical, probe(2))
            .unwrap();
        assert_eq!(s.cancel_owner(entity), 1);
        let drain = s.drain_due(due, WorkLane::Critical);
        assert_eq!(drain.fired.len(), 1);
        assert_eq!(drain.fired[0].id, world_job);
        assert!(s.get(owned).is_none());
    }

    #[test]
    fn schedule_during_drain_does_not_recurse_same_pass() {
        let mut s = Scheduler::new();
        let now = SimulationTick::from_count(1);
        s.schedule_at(now, ScheduleOwner::World, WorkLane::Critical, probe(1));
        let drain = s.drain_due(now, WorkLane::Critical);
        assert_eq!(drain.fired.len(), 1);
        s.schedule_at(now, ScheduleOwner::World, WorkLane::Critical, probe(2));
        assert_eq!(s.due_count(now, WorkLane::Critical), 1);
        let again = s.drain_due(now, WorkLane::Critical);
        assert_eq!(again.fired.len(), 1);
        assert_eq!(again.fired[0].kind, probe(2));
    }

    #[test]
    fn deferred_budget_preserves_remainder_and_progresses() {
        let mut s = Scheduler::new();
        let now = SimulationTick::from_count(1);
        let n = DEFERRED_DRAIN_BUDGET + 10;
        for i in 0..n {
            s.schedule_at(now, ScheduleOwner::World, WorkLane::Deferred, probe(i));
        }
        let first = s.drain_due(now, WorkLane::Deferred);
        assert_eq!(first.fired.len(), DEFERRED_DRAIN_BUDGET as usize);
        assert!(first.budget_exhausted);
        assert_eq!(first.remaining_due, 10);
        let second = s.drain_due(now, WorkLane::Deferred);
        assert_eq!(second.fired.len(), 10);
        assert!(!second.budget_exhausted);
        assert_eq!(s.live_count(), 0);
    }

    #[test]
    fn deferred_does_not_starve() {
        let mut s = Scheduler::new();
        let now = SimulationTick::from_count(4);
        let n = DEFERRED_DRAIN_BUDGET * 3 + 1;
        for i in 0..n {
            s.schedule_at(now, ScheduleOwner::World, WorkLane::Deferred, probe(i));
        }
        let mut seen = 0u32;
        let mut passes = 0u32;
        while s.live_count() > 0 {
            let drain = s.drain_due(now, WorkLane::Deferred);
            seen = seen.saturating_add(u32::try_from(drain.fired.len()).unwrap());
            passes = passes.saturating_add(1);
            assert!(!drain.fired.is_empty());
            assert!(passes <= n.div_ceil(DEFERRED_DRAIN_BUDGET.max(1)) + 1);
        }
        assert_eq!(seen, n);
    }

    #[test]
    fn critical_and_deferred_are_separate_lanes() {
        let mut s = Scheduler::new();
        let now = SimulationTick::ZERO;
        s.schedule_at(now, ScheduleOwner::World, WorkLane::Critical, probe(1));
        s.schedule_at(now, ScheduleOwner::World, WorkLane::Deferred, probe(2));
        let crit = s.drain_due(now, WorkLane::Critical);
        assert_eq!(crit.fired.len(), 1);
        assert_eq!(s.due_count(now, WorkLane::Deferred), 1);
        let def = s.drain_due(now, WorkLane::Deferred);
        assert_eq!(def.fired.len(), 1);
    }

    #[test]
    fn full_queue_rejects_new_work() {
        let mut s = Scheduler::new();
        let due = SimulationTick::from_count(9);
        for i in 0..SCHEDULER_CAPACITY {
            assert!(
                s.schedule_at(due, ScheduleOwner::World, WorkLane::Deferred, probe(i),)
                    .is_some()
            );
        }
        assert!(
            s.schedule_at(due, ScheduleOwner::World, WorkLane::Deferred, probe(99))
                .is_none()
        );
    }

    #[test]
    fn critical_ceiling_is_overload_safeguard_not_unlimited_drain() {
        let mut s = Scheduler::new();
        let now = SimulationTick::from_count(2);
        let n = CRITICAL_DRAIN_CEILING + 3;
        for i in 0..n {
            s.schedule_at(now, ScheduleOwner::World, WorkLane::Critical, probe(i));
        }
        let first = s.drain_due(now, WorkLane::Critical);
        assert_eq!(first.fired.len(), CRITICAL_DRAIN_CEILING as usize);
        assert!(first.ceiling_hit);
        assert_eq!(first.remaining_due, 3);
        assert_eq!(s.critical_ceiling_hits(), 1);
        let second = s.drain_due(now, WorkLane::Critical);
        assert_eq!(second.fired.len(), 3);
        assert!(!second.ceiling_hit);
        assert_eq!(
            s.critical_executed_total(),
            u64::from(CRITICAL_DRAIN_CEILING + 3)
        );
    }

    #[test]
    fn execution_totals_count_schedule_fire_and_cancel() {
        let mut s = Scheduler::new();
        let now = SimulationTick::from_count(1);
        let keep = s
            .schedule_at(now, ScheduleOwner::World, WorkLane::Critical, probe(1))
            .unwrap();
        let drop = s
            .schedule_at(now, ScheduleOwner::World, WorkLane::Deferred, probe(2))
            .unwrap();
        assert_eq!(s.scheduled_total(), 2);
        assert!(s.cancel(drop));
        assert_eq!(s.cancelled_total(), 1);
        let _ = keep;
        let crit = s.drain_due(now, WorkLane::Critical);
        assert_eq!(crit.fired.len(), 1);
        assert_eq!(s.critical_executed_total(), 1);
        assert_eq!(s.deferred_executed_total(), 0);
        assert_eq!(s.live_count(), 0);
    }
}
