//! Component/domain dirty flags for future delta replication.
//!
//! Not a single `entity_dirty: bool`. Not a scheduler.

/// Which replicated domains changed since last consume.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirtyFlags {
    pub transform: bool,
    pub health: bool,
    pub membership: bool,
    pub replication: bool,
}

impl DirtyFlags {
    #[must_use]
    pub const fn any(self) -> bool {
        self.transform || self.health || self.membership || self.replication
    }

    pub fn merge(&mut self, other: Self) {
        self.transform |= other.transform;
        self.health |= other.health;
        self.membership |= other.membership;
        self.replication |= other.replication;
    }

    pub fn take(&mut self) -> Self {
        core::mem::take(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_resets() {
        let mut flags = DirtyFlags {
            transform: true,
            ..DirtyFlags::default()
        };
        assert!(flags.any());
        let taken = flags.take();
        assert!(taken.transform);
        assert!(!flags.any());
    }
}
