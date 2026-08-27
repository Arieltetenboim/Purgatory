//! Surface / approach blocking policy for FOOTNOTE.

use crate::entity::EntityId;
use crate::platform::{Approach, Platform, PlatformKind};

/// Motion history used when deciding whether a platform blocks this approach.
#[derive(Clone, Copy, Debug)]
pub struct BlockQuery {
    pub approach: Approach,
    pub platform_id: EntityId,
    /// Player bottom before this axis integration.
    pub previous_bottom: f32,
    pub platform_top: f32,
    pub ignored_platform: Option<EntityId>,
}

/// Whether this platform should block the given approach under FOOTNOTE rules.
///
/// Downward landings (Solid and OneWay) require a top-surface approach from above.
/// Geometric overlap alone is not enough.
#[must_use]
pub fn surface_blocks(platform: Platform, query: BlockQuery) -> bool {
    if query.ignored_platform == Some(query.platform_id) {
        return false;
    }
    match platform.kind {
        PlatformKind::Solid => match query.approach {
            Approach::Down => query.previous_bottom >= query.platform_top - 1e-4,
            Approach::Up | Approach::Left | Approach::Right => true,
            Approach::None => false,
        },
        PlatformKind::OneWay => match query.approach {
            Approach::Down => {
                // Land only when descending across the top surface from above.
                query.previous_bottom >= query.platform_top - 1e-4
            }
            Approach::Left | Approach::Right | Approach::Up | Approach::None => false,
        },
    }
}
