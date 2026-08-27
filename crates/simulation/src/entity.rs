//! Runtime entity identity.
//!
//! [`EntityId`] is a cheap, copyable handle. It is **not** a content ID and
//! **not** a raw slot index. A generation field rejects stale IDs after a
//! slot is reused. Do not serialize this onto the network in Phase 4.

use std::fmt;

/// Runtime instance identity. Temporary; owned by a [`crate::World`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    pub(crate) const fn new(index: u32, generation: u32) -> Self {
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

/// Explicit runtime classification. Later kinds are added when those systems exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKind {
    Player,
    Platform,
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Player => f.write_str("Player"),
            Self::Platform => f.write_str("Platform"),
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
    }
}
