//! Runtime entity identity.
//!
//! [`EntityId`] is a cheap, copyable handle. It is **not** a content ID and
//! **not** a raw slot index. A generation field rejects stale IDs after a
//! slot is reused. Network snapshots copy index + generation as identity.

use std::fmt;

/// Runtime instance identity. Temporary; owned by a [`crate::World`].
///
/// Alias [`RuntimeEntityId`] documents the Phase 6 identity domain. Do not use
/// this as a content id or persistent id.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    pub(crate) const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Reconstruct a wire-delivered identity. Generation is part of equality.
    #[must_use]
    pub const fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Storage slot. Not a stable identity by itself.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Generation that must match the live slot.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.index, self.generation)
    }
}

/// Phase 6 name for [`EntityId`]. Same generational handle; not a rename of
/// public APIs.
pub type RuntimeEntityId = EntityId;

/// Explicit runtime classification. Derived from capabilities in Phase 6A.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKind {
    Player,
    Platform,
    /// Composed entity without player or platform capability.
    Generic,
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Player => f.write_str("Player"),
            Self::Platform => f.write_str("Platform"),
            Self::Generic => f.write_str("Generic"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_with_different_generations_are_not_equal() {
        let a = EntityId::new(3, 1);
        let b = EntityId::new(3, 2);
        assert_ne!(a, b);
        assert_eq!(a.index(), b.index());
        assert_eq!(a.to_string(), "3:1");
        assert_eq!(EntityKind::Player.to_string(), "Player");
        assert_eq!(EntityKind::Platform.to_string(), "Platform");
        assert_eq!(EntityKind::Generic.to_string(), "Generic");
    }
}
