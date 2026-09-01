//! Replication dirty fan-out accounting (Phase 6G.7B).

use purgatory_common::{IntDistribution, ReplicationFanoutSnapshot};

/// Rolling sampler for capacity artifacts.
#[derive(Debug)]
pub struct ReplicationFanoutAccounting {
    scanned_ring: Vec<u32>,
    present_ring: Vec<u32>,
    cap: usize,
    publish_passes: u64,
    dirty_entities_total: u64,
    dirty_transform_total: u64,
    dirty_health_total: u64,
    known_relationships_present_total: u64,
    known_relationships_scanned_total: u64,
    interested_observers_total: u64,
    updates_emitted_total: u64,
    serialize_attempts_total: u64,
    budget_deferred_total: u64,
    cadence_deferred_total: u64,
    recovery_rescues_total: u64,
    policy_eligible_total: u64,
    policy_domain_suppressed_total: u64,
    priority_deferred_total: u64,
    state_coalesced_total: u64,
    bytes_emitted_total: u64,
}

impl Default for ReplicationFanoutAccounting {
    fn default() -> Self {
        Self::new(512)
    }
}

impl ReplicationFanoutAccounting {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            scanned_ring: Vec::with_capacity(cap),
            present_ring: Vec::with_capacity(cap),
            cap: cap.max(1),
            publish_passes: 0,
            dirty_entities_total: 0,
            dirty_transform_total: 0,
            dirty_health_total: 0,
            known_relationships_present_total: 0,
            known_relationships_scanned_total: 0,
            interested_observers_total: 0,
            updates_emitted_total: 0,
            serialize_attempts_total: 0,
            budget_deferred_total: 0,
            cadence_deferred_total: 0,
            recovery_rescues_total: 0,
            policy_eligible_total: 0,
            policy_domain_suppressed_total: 0,
            priority_deferred_total: 0,
            state_coalesced_total: 0,
            bytes_emitted_total: 0,
        }
    }

    pub fn note_dirty_pass(
        &mut self,
        dirty_entities: u32,
        dirty_transform: u32,
        dirty_health: u32,
        interested_observers: u32,
    ) {
        self.dirty_entities_total = self
            .dirty_entities_total
            .saturating_add(u64::from(dirty_entities));
        self.dirty_transform_total = self
            .dirty_transform_total
            .saturating_add(u64::from(dirty_transform));
        self.dirty_health_total = self
            .dirty_health_total
            .saturating_add(u64::from(dirty_health));
        self.interested_observers_total = self
            .interested_observers_total
            .saturating_add(u64::from(interested_observers));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn note_observer_publish(
        &mut self,
        known_present: u32,
        known_scanned: u32,
        updates: u32,
        serialize_attempts: u32,
        budget_deferred: u32,
        cadence_deferred: u32,
        recovery_rescues: u32,
    ) {
        self.publish_passes = self.publish_passes.saturating_add(1);
        self.known_relationships_present_total = self
            .known_relationships_present_total
            .saturating_add(u64::from(known_present));
        self.known_relationships_scanned_total = self
            .known_relationships_scanned_total
            .saturating_add(u64::from(known_scanned));
        self.updates_emitted_total = self
            .updates_emitted_total
            .saturating_add(u64::from(updates));
        self.serialize_attempts_total = self
            .serialize_attempts_total
            .saturating_add(u64::from(serialize_attempts));
        self.budget_deferred_total = self
            .budget_deferred_total
            .saturating_add(u64::from(budget_deferred));
        self.cadence_deferred_total = self
            .cadence_deferred_total
            .saturating_add(u64::from(cadence_deferred));
        self.recovery_rescues_total = self
            .recovery_rescues_total
            .saturating_add(u64::from(recovery_rescues));
        if self.scanned_ring.len() >= self.cap {
            let drop_at = self.scanned_ring.len() / 2;
            self.scanned_ring.drain(0..drop_at);
            self.present_ring
                .drain(0..drop_at.min(self.present_ring.len()));
        }
        self.scanned_ring.push(known_scanned);
        self.present_ring.push(known_present);
    }

    pub fn note_policy(
        &mut self,
        eligible: u32,
        domain_suppressed: u32,
        priority_deferred: u32,
        state_coalesced: u32,
        bytes: u32,
    ) {
        self.policy_eligible_total = self
            .policy_eligible_total
            .saturating_add(u64::from(eligible));
        self.policy_domain_suppressed_total = self
            .policy_domain_suppressed_total
            .saturating_add(u64::from(domain_suppressed));
        self.priority_deferred_total = self
            .priority_deferred_total
            .saturating_add(u64::from(priority_deferred));
        self.state_coalesced_total = self
            .state_coalesced_total
            .saturating_add(u64::from(state_coalesced));
        self.bytes_emitted_total = self.bytes_emitted_total.saturating_add(u64::from(bytes));
    }

    #[must_use]
    pub fn snapshot(&self) -> ReplicationFanoutSnapshot {
        ReplicationFanoutSnapshot {
            schema: ReplicationFanoutSnapshot::SCHEMA,
            publish_passes: self.publish_passes,
            dirty_entities_total: self.dirty_entities_total,
            dirty_transform_total: self.dirty_transform_total,
            dirty_health_total: self.dirty_health_total,
            known_relationships_present_total: self.known_relationships_present_total,
            known_relationships_scanned_total: self.known_relationships_scanned_total,
            interested_observers_total: self.interested_observers_total,
            updates_emitted_total: self.updates_emitted_total,
            serialize_attempts_total: self.serialize_attempts_total,
            budget_deferred_total: self.budget_deferred_total,
            cadence_deferred_total: self.cadence_deferred_total,
            recovery_rescues_total: self.recovery_rescues_total,
            scanned_per_pass: dist(&self.scanned_ring),
            known_present_per_pass: dist(&self.present_ring),
            policy_eligible_total: self.policy_eligible_total,
            policy_domain_suppressed_total: self.policy_domain_suppressed_total,
            priority_deferred_total: self.priority_deferred_total,
            state_coalesced_total: self.state_coalesced_total,
            bytes_emitted_total: self.bytes_emitted_total,
        }
    }
}

fn dist(samples: &[u32]) -> IntDistribution {
    if samples.is_empty() {
        return IntDistribution::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let sum: u64 = sorted.iter().map(|v| u64::from(*v)).sum();
    let mean = sum as f64 / n as f64;
    IntDistribution {
        mean,
        p50: percentile(&sorted, 0.50),
        p95: percentile(&sorted, 0.95),
        p99: percentile(&sorted, 0.99),
        max: u64::from(*sorted.last().unwrap_or(&0)),
        sample_count: n as u64,
    }
}

fn percentile(sorted: &[u32], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    f64::from(sorted[idx.min(sorted.len() - 1)])
}
