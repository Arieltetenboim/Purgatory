//! Centralized contact / crossing tolerance for FOOTNOTE collision.
//!
//! World units are on the order of 0.1–24 for platforms and ~0.8×1.2 for the
//! player. A 1e-3 (0.001) epsilon is invisible at that scale but larger than
//! typical f32 edge noise (~1e-7) that previously made flush underside contact
//! still report as AABB overlap.
//!
//! **Touching a surface within [`CONTACT_EPSILON`] is not penetration.**

/// Shared contact / crossing tolerance (world units).
pub const CONTACT_EPSILON: f32 = 1e-3;

/// Maximum translation applied by exceptional penetration recovery (world units).
pub const MAX_RECOVERY_TRANSLATION: f32 = 0.5;

/// Meaningful penetration depth on one axis before recovery may run.
pub const RECOVERY_PENETRATION_MIN: f32 = CONTACT_EPSILON * 4.0;

/// True when two AABBs overlap with penetration deeper than [`CONTACT_EPSILON`]
/// on **both** axes (not mere surface touch).
#[must_use]
pub fn penetrates(a: crate::aabb::Aabb, b: crate::aabb::Aabb) -> bool {
    overlap_x(a, b) > CONTACT_EPSILON && overlap_y(a, b) > CONTACT_EPSILON
}

#[must_use]
pub fn overlap_x(a: crate::aabb::Aabb, b: crate::aabb::Aabb) -> f32 {
    (a.max_x().min(b.max_x()) - a.min_x().max(b.min_x())).max(0.0)
}

#[must_use]
pub fn overlap_y(a: crate::aabb::Aabb, b: crate::aabb::Aabb) -> f32 {
    (a.max_y().min(b.max_y()) - a.min_y().max(b.min_y())).max(0.0)
}
