//! Server-side interest-policy rectangles.
//!
//! Interest is a **validated visible-view envelope + prefetch margin**, derived
//! from the observer's authoritative pose, map camera limits (`WorldBounds`),
//! and the FOOTNOTE logical viewport. The client camera is not a network
//! authority; a modified client cannot request an arbitrary distant region.
//!
//! Previous model (Phase 6D): player-centered `[16, 9]` half-extents. That did
//! not cover a clamped camera viewport (`~24.89×14` plus Dead Zone offset).

use crate::aabb::Aabb;
use crate::bounds::WorldBounds;
use crate::stage::FOOTNOTE_TEST_VIEWPORT_HEIGHT;

/// Logical viewport aspect matching the FOOTNOTE client camera (`16:9`).
pub const AOI_VIEWPORT_ASPECT: f32 = 16.0 / 9.0;

/// Camera Dead Zone half-extents used to bound legal camera centers.
/// Must match [`apps/client` `camera_follow::DEAD_ZONE_HALF_*`].
pub const AOI_CAMERA_DEAD_ZONE_HALF: [f32; 2] = [3.0, 4.0];

/// Extra world units added around the visible envelope for Enter (prefetch).
/// Entities replicate before they reach the visible edge.
pub const AOI_PREFETCH_MARGIN: f32 = 2.0;

/// Extra half-extent added on each axis for the leave rectangle (hysteresis).
/// Leave only after the entity is this far outside the enter envelope.
pub const AOI_LEAVE_MARGIN: f32 = 2.0;

/// Unclamped enter half-extents in map interior:
/// `viewport/2 + dead zone + prefetch`. Not a player-centered radius.
pub const AOI_POLICY_HALF_EXTENTS: [f32; 2] = [
    FOOTNOTE_TEST_VIEWPORT_HEIGHT * AOI_VIEWPORT_ASPECT * 0.5
        + AOI_CAMERA_DEAD_ZONE_HALF[0]
        + AOI_PREFETCH_MARGIN,
    FOOTNOTE_TEST_VIEWPORT_HEIGHT * 0.5 + AOI_CAMERA_DEAD_ZONE_HALF[1] + AOI_PREFETCH_MARGIN,
];

/// Conservative half-extents for inverse observer invalidation (6G.5).
///
/// Any observer who could gain/lose an entity at pose `P` under leave policy
/// must lie inside expand(`P`, these halves). Sized from FOOTNOTE measured max
/// distance from an observer to a point in their leave rect (edge clamp makes
/// this larger than [`AOI_POLICY_HALF_EXTENTS`] + leave margin). Guarded by
/// `measure_max_leave_extent_from_observer` / `influence_covers_leave_inverse_on_footnote`.
pub const AOI_INFLUENCE_HALF_EXTENTS: [f32; 2] = [28.5, 17.6];

/// Enter/leave policy rectangles for one observer pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AoiRects {
    pub enter: Aabb,
    pub leave: Aabb,
}

/// FOOTNOTE logical viewport size (world units). Matches client camera height
/// [`FOOTNOTE_TEST_VIEWPORT_HEIGHT`] and 16:9 width.
#[must_use]
pub fn aoi_viewport_size() -> [f32; 2] {
    let height = FOOTNOTE_TEST_VIEWPORT_HEIGHT;
    [height * AOI_VIEWPORT_ASPECT, height]
}

/// Clamp a desired camera center so the viewport stays inside `bounds`.
/// Must match client `renderer::clamp_camera_center`.
#[must_use]
pub fn aoi_clamp_camera_center(
    desired: [f32; 2],
    viewport_width: f32,
    viewport_height: f32,
    bounds: WorldBounds,
) -> [f32; 2] {
    let half_w = viewport_width * 0.5;
    let half_h = viewport_height * 0.5;
    let x = if bounds.width() <= viewport_width {
        bounds.center()[0]
    } else {
        desired[0].clamp(bounds.min_x + half_w, bounds.max_x - half_w)
    };
    let y = if bounds.height() <= viewport_height {
        bounds.center()[1]
    } else {
        desired[1].clamp(bounds.min_y + half_h, bounds.max_y - half_h)
    };
    [x, y]
}

/// Union of legal client viewports for `position` (Dead Zone + clamp).
#[must_use]
pub fn aoi_view_envelope(position: [f32; 2], bounds: WorldBounds) -> Aabb {
    let [vw, vh] = aoi_viewport_size();
    let dz = AOI_CAMERA_DEAD_ZONE_HALF;
    let cam_lo =
        aoi_clamp_camera_center([position[0] - dz[0], position[1] - dz[1]], vw, vh, bounds);
    let cam_hi =
        aoi_clamp_camera_center([position[0] + dz[0], position[1] + dz[1]], vw, vh, bounds);
    let half_w = vw * 0.5;
    let half_h = vh * 0.5;
    Aabb::from_min_max(
        cam_lo[0].min(cam_hi[0]) - half_w,
        cam_lo[1].min(cam_hi[1]) - half_h,
        cam_lo[0].max(cam_hi[0]) + half_w,
        cam_lo[1].max(cam_hi[1]) + half_h,
    )
}

/// Build enter/leave rects from the view envelope. No wrap. Clamped to `bounds`.
#[must_use]
pub fn aoi_policy_rects(position: [f32; 2], bounds: WorldBounds) -> AoiRects {
    let view = aoi_view_envelope(position, bounds);
    let enter = clamp_aabb(expand_aabb(view, AOI_PREFETCH_MARGIN), bounds);
    let leave = clamp_aabb(
        expand_aabb(view, AOI_PREFETCH_MARGIN + AOI_LEAVE_MARGIN),
        bounds,
    );
    AoiRects { enter, leave }
}

#[must_use]
pub fn point_in_aabb(position: [f32; 2], aabb: Aabb) -> bool {
    aabb.contains_point(position)
}

fn expand_aabb(aabb: Aabb, margin: f32) -> Aabb {
    Aabb::from_min_max(
        aabb.min_x() - margin,
        aabb.min_y() - margin,
        aabb.max_x() + margin,
        aabb.max_y() + margin,
    )
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
    use crate::stage::FOOTNOTE_SPAWN_X;

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

    #[test]
    fn left_clamp_enter_covers_visible_right_plus_prefetch() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let player = [FOOTNOTE_SPAWN_X, -3.0];
        let [vw, vh] = aoi_viewport_size();
        let cam = aoi_clamp_camera_center(player, vw, vh, bounds);
        let view_right = cam[0] + vw * 0.5;
        let rects = aoi_policy_rects(player, bounds);
        assert!(
            (rects.enter.max_x() - (view_right + AOI_PREFETCH_MARGIN)).abs() < 1e-3,
            "enter max_x={} view_right={} prefetch={}",
            rects.enter.max_x(),
            view_right,
            AOI_PREFETCH_MARGIN
        );
        assert!(
            point_in_aabb([view_right - 0.05, cam[1]], rects.enter),
            "visible right edge must be inside enter"
        );
        let old_player_centered_right = player[0] + 16.0;
        assert!(
            view_right > old_player_centered_right + 0.5,
            "runtime evidence: clamped view extends past player-centered 16 wu"
        );
    }

    #[test]
    fn right_clamp_enter_covers_visible_left_plus_prefetch() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let player = [bounds.max_x - 0.4, -3.0];
        let [vw, vh] = aoi_viewport_size();
        let cam = aoi_clamp_camera_center(player, vw, vh, bounds);
        let view_left = cam[0] - vw * 0.5;
        let rects = aoi_policy_rects(player, bounds);
        assert!(point_in_aabb([view_left + 0.05, cam[1]], rects.enter));
        assert!(rects.enter.min_x() <= view_left + 1e-3);
    }

    #[test]
    fn interest_is_not_the_entire_map() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let rects = aoi_policy_rects([0.0, 0.0], bounds);
        assert!(rects.leave.size()[0] < bounds.width() - 1.0);
        assert!(rects.leave.size()[1] <= bounds.height() + 1e-3);
    }

    #[test]
    fn measure_max_leave_extent_from_observer() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut max_dx = 0.0f32;
        let mut max_dy = 0.0f32;
        let step = 0.5;
        let mut ox = bounds.min_x + 0.5;
        while ox <= bounds.max_x - 0.5 {
            let mut oy = bounds.min_y + 0.5;
            while oy <= bounds.max_y - 0.5 {
                let rects = aoi_policy_rects([ox, oy], bounds);
                max_dx = max_dx
                    .max((rects.leave.max_x() - ox).abs())
                    .max((rects.leave.min_x() - ox).abs());
                max_dy = max_dy
                    .max((rects.leave.max_y() - oy).abs())
                    .max((rects.leave.min_y() - oy).abs());
                oy += step;
            }
            ox += step;
        }
        eprintln!("FOOTNOTE max leave extent dx={max_dx} dy={max_dy}");
        assert!(
            AOI_INFLUENCE_HALF_EXTENTS[0] + 1e-3 >= max_dx,
            "influence x {} < max leave extent {}",
            AOI_INFLUENCE_HALF_EXTENTS[0],
            max_dx
        );
        assert!(
            AOI_INFLUENCE_HALF_EXTENTS[1] + 1e-3 >= max_dy,
            "influence y {} < max leave extent {}",
            AOI_INFLUENCE_HALF_EXTENTS[1],
            max_dy
        );
    }

    #[test]
    fn influence_covers_leave_inverse_on_footnote() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let half = AOI_INFLUENCE_HALF_EXTENTS;
        let step = 2.0;
        let mut ox = bounds.min_x + 0.5;
        while ox <= bounds.max_x - 0.5 {
            let mut oy = bounds.min_y + 0.5;
            while oy <= bounds.max_y - 0.5 {
                let rects = aoi_policy_rects([ox, oy], bounds);
                let mut ex = bounds.min_x + 0.5;
                while ex <= bounds.max_x - 0.5 {
                    let mut ey = bounds.min_y + 0.5;
                    while ey <= bounds.max_y - 0.5 {
                        if point_in_aabb([ex, ey], rects.leave) {
                            assert!(
                                (ox - ex).abs() <= half[0] + 1e-3
                                    && (oy - ey).abs() <= half[1] + 1e-3,
                                "observer ({ox},{oy}) sees entity ({ex},{ey}) in leave but outside influence"
                            );
                        }
                        ey += step;
                    }
                    ex += step;
                }
                oy += step;
            }
            ox += step;
        }
    }

    #[test]
    fn prefetch_is_documented_two_wu() {
        assert!((AOI_PREFETCH_MARGIN - 2.0).abs() < f32::EPSILON);
        assert!((AOI_LEAVE_MARGIN - 2.0).abs() < f32::EPSILON);
    }
}
