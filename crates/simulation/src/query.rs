//! Gameplay spatial query boundary.
//!
//! Implementations live on [`crate::World`] (`query_aabb`, `query_radius`,
//! `entities_near`). The uniform grid is replaceable behind those methods.
//! Queries take a [`WorldAddress`] and never cross Map/Channel/Instance unless
//! the caller passes a different address.

use crate::entity::EntityKind;

/// Optional predicate applied after the spatial candidate set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryFilter {
    Any,
    Kind(EntityKind),
    HasHealth,
    HasInteractable,
}

/// Optional cap on returned entities (slot-index order from the grid).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryLimit {
    pub max: usize,
}

impl QueryLimit {
    #[must_use]
    pub const fn max(max: usize) -> Self {
        Self { max }
    }
}
