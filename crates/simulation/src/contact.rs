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

/// True when two AABBs overlap or their surfaces touch within
/// [`CONTACT_EPSILON`] on both axes.
///
/// This is a gameplay-contact query, not a penetration query. It must not be
/// used by normal collision response or depenetration.
#[must_use]
pub fn touches_or_overlaps(a: crate::aabb::Aabb, b: crate::aabb::Aabb) -> bool {
    a.min_x() <= b.max_x() + CONTACT_EPSILON
        && a.max_x() + CONTACT_EPSILON >= b.min_x()
        && a.min_y() <= b.max_y() + CONTACT_EPSILON
        && a.max_y() + CONTACT_EPSILON >= b.min_y()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aabb::Aabb;

    #[test]
    fn gameplay_contact_includes_exact_surface_touch() {
        let a = Aabb::new([0.0, 0.0], [0.4, 0.6]);
        let touching = Aabb::new([0.8, 0.0], [0.4, 0.6]);
        let separated = Aabb::new([0.8 + CONTACT_EPSILON * 2.0, 0.0], [0.4, 0.6]);

        assert!(touches_or_overlaps(a, touching));
        assert!(!touches_or_overlaps(a, separated));
        assert!(!a.overlaps(touching));
    }
}
