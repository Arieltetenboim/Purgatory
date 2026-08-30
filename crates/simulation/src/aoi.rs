//! Server-side interest-policy rectangles.
//!
//! These constants are **not** an authoritative reflection of client 16:9
//! resolution, camera clamp, or [`crate::stage::FOOTNOTE_TEST_VIEWPORT_HEIGHT`].
//! They are initial development interest extents.

use crate::aabb::Aabb;
use crate::bounds::WorldBounds;

/// Half-extents of the enter rectangle (world units). Initial dev constant.
pub const AOI_POLICY_HALF_EXTENTS: [f32; 2] = [16.0, 9.0];

/// Extra half-extent added on each axis for the leave rectangle.
pub const AOI_LEAVE_MARGIN: f32 = 2.0;

/// Enter/leave policy rectangles for one observer pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AoiRects {
    pub enter: Aabb,
    pub leave: Aabb,
}

/// Build policy rects centered on `position`, clamped to `bounds`. No wrap.
#[must_use]
pub fn aoi_policy_rects(position: [f32; 2], bounds: WorldBounds) -> AoiRects {
    let enter = clamp_aabb(Aabb::new(position, AOI_POLICY_HALF_EXTENTS), bounds);
    let leave = clamp_aabb(
        Aabb::new(
            position,
            [
                AOI_POLICY_HALF_EXTENTS[0] + AOI_LEAVE_MARGIN,
                AOI_POLICY_HALF_EXTENTS[1] + AOI_LEAVE_MARGIN,
            ],
        ),
        bounds,
    );
    AoiRects { enter, leave }
}

#[must_use]
pub fn point_in_aabb(position: [f32; 2], aabb: Aabb) -> bool {
    aabb.contains_point(position)
}

fn clamp_aabb(aabb: Aabb, bounds: WorldBounds) -> Aabb {
    let min_x = aabb.min_x().max(bounds.min_x);
    let max_x = aabb.max_x().min(bounds.max_x);
    let min_y = aabb.min_y().max(bounds.min_y);
    let max_y = aabb.max_y().min(bounds.max_y);
    if min_x >= max_x || min_y >= max_y {
        return Aabb::new(aabb.center, [0.0, 0.0]);
    }
    Aabb::from_min_max(min_x, min_y, max_x, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leave_is_larger_than_enter() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let rects = aoi_policy_rects([0.0, 0.0], bounds);
        assert!(rects.leave.half_extents[0] >= rects.enter.half_extents[0]);
        assert!(rects.leave.half_extents[1] >= rects.enter.half_extents[1]);
    }

    #[test]
    fn rects_do_not_extend_past_map_bounds() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let rects = aoi_policy_rects([bounds.max_x, 0.0], bounds);
        assert!(rects.enter.max_x() <= bounds.max_x + f32::EPSILON);
        assert!(rects.leave.max_x() <= bounds.max_x + f32::EPSILON);
    }
}
