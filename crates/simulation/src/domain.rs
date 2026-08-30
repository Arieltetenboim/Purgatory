//! Per-domain change revisions for multi-observer replication.
//!
//! Networking must not consume a global dirty bit. Each observer compares
//! [`DomainRevs`] against its own `last_committed_rev`.

/// Monotonic per-domain generations. Increment only when the replicated value changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DomainRevs {
    pub transform: u64,
    pub health: u64,
    pub membership: u64,
    pub replication: u64,
}

impl DomainRevs {
    #[must_use]
    pub const fn initial_transform() -> Self {
        Self {
            transform: 1,
            ..Self::zero()
        }
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self {
            transform: 0,
            health: 0,
            membership: 0,
            replication: 0,
        }
    }

    pub fn bump_transform(&mut self) {
        self.transform = self.transform.saturating_add(1);
    }

    pub fn bump_health(&mut self) {
        self.health = self.health.saturating_add(1);
    }

    pub fn bump_membership(&mut self) {
        self.membership = self.membership.saturating_add(1);
    }

    pub fn bump_replication(&mut self) {
        self.replication = self.replication.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_does_not_reset() {
        let mut revs = DomainRevs::initial_transform();
        revs.bump_transform();
        revs.bump_health();
        assert_eq!(revs.transform, 2);
        assert_eq!(revs.health, 1);
    }
}
