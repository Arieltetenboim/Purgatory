//! Runtime entity lifecycle. Observer relevance is not a lifecycle state.

/// Stored lifecycle of a live (or left-world) slot occupant.
///
/// `not visible to observer X` is **not** despawn. Changing address is an
/// operation on an `Active` (or re-entering) entity, not destruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EntityLifecycle {
    /// Allocated, in a [`crate` world address], eligible for queries.
    Active,
    /// Still allocated; not in world queries / relevance. Not despawned.
    LeftWorld,
}

impl std::fmt::Display for EntityLifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => f.write_str("Active"),
            Self::LeftWorld => f.write_str("LeftWorld"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_is_not_left_world() {
        assert_ne!(EntityLifecycle::Active, EntityLifecycle::LeftWorld);
        assert_eq!(EntityLifecycle::Active.to_string(), "Active");
    }
}
