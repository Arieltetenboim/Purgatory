//! AOI interest-invalidation locality accounting (Phase 6G.6 / 6G.7A).

use purgatory_common::{IntDistribution, InterestLocalitySnapshot};

/// Rolling sampler used by [`crate::World`].
#[derive(Debug)]
pub struct InterestLocalityAccounting {
    ring: Vec<u32>,
    prefilter_ring: Vec<u32>,
    cap: usize,
    invalidation_events: u64,
    cell_boundary_crossings: u64,
    same_cell_moves: u64,
    cells_touched_total: u64,
    observers_dirtied_total: u64,
    entities_moved_total: u64,
    influence_prefilter_total: u64,
    membership_xor_total: u64,
}

impl Default for InterestLocalityAccounting {
    fn default() -> Self {
        Self::new(512)
    }
}

impl InterestLocalityAccounting {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            ring: Vec::with_capacity(cap),
            prefilter_ring: Vec::with_capacity(cap),
            cap: cap.max(1),
            invalidation_events: 0,
            cell_boundary_crossings: 0,
            same_cell_moves: 0,
            cells_touched_total: 0,
            observers_dirtied_total: 0,
            entities_moved_total: 0,
            influence_prefilter_total: 0,
            membership_xor_total: 0,
        }
    }

    pub fn record_invalidation(
        &mut self,
        observers_dirtied: u32,
        cell_crossed: bool,
        cells_touched: u32,
        entity_moved: bool,
        influence_prefilter: u32,
        membership_xor: u32,
    ) {
        self.invalidation_events = self.invalidation_events.saturating_add(1);
        if cell_crossed {
            self.cell_boundary_crossings = self.cell_boundary_crossings.saturating_add(1);
        } else {
            self.same_cell_moves = self.same_cell_moves.saturating_add(1);
        }
        self.cells_touched_total = self
            .cells_touched_total
            .saturating_add(u64::from(cells_touched));
        self.observers_dirtied_total = self
            .observers_dirtied_total
            .saturating_add(u64::from(observers_dirtied));
        self.influence_prefilter_total = self
            .influence_prefilter_total
            .saturating_add(u64::from(influence_prefilter));
        self.membership_xor_total = self
            .membership_xor_total
            .saturating_add(u64::from(membership_xor));
        if entity_moved {
            self.entities_moved_total = self.entities_moved_total.saturating_add(1);
        }
        if self.ring.len() >= self.cap {
            let drop_at = self.ring.len() / 2;
            self.ring.drain(0..drop_at);
            self.prefilter_ring
                .drain(0..drop_at.min(self.prefilter_ring.len()));
        }
        self.ring.push(observers_dirtied);
        self.prefilter_ring.push(influence_prefilter);
    }

    #[must_use]
    pub fn snapshot(&self) -> InterestLocalitySnapshot {
        InterestLocalitySnapshot {
            schema: InterestLocalitySnapshot::SCHEMA,
            invalidation_events: self.invalidation_events,
            cell_boundary_crossings: self.cell_boundary_crossings,
            same_cell_moves: self.same_cell_moves,
            cells_touched_total: self.cells_touched_total,
            observers_dirtied: distribution_from(&self.ring),
            observers_dirtied_total: self.observers_dirtied_total,
            entities_moved_total: self.entities_moved_total,
            influence_prefilter_total: self.influence_prefilter_total,
            membership_xor_total: self.membership_xor_total,
            influence_prefilter: distribution_from(&self.prefilter_ring),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.cap);
    }
}

fn distribution_from(samples: &[u32]) -> IntDistribution {
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
    let n = sorted.len();
    let idx = ((p * (n.saturating_sub(1)) as f64).round() as usize).min(n - 1);
    f64::from(sorted[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_tracks_max() {
        let mut a = InterestLocalityAccounting::new(16);
        a.record_invalidation(1, false, 1, true, 20, 0);
        a.record_invalidation(8, true, 2, true, 30, 7);
        let s = a.snapshot();
        assert_eq!(s.observers_dirtied.max, 8);
        assert_eq!(s.entities_moved_total, 2);
        assert_eq!(s.cell_boundary_crossings, 1);
        assert_eq!(s.membership_xor_total, 7);
        assert_eq!(s.influence_prefilter_total, 50);
    }
}
