//! Client presentation camera: Dead Zone containment + exponential follow.
//!
//! The Dead Zone is a free-movement box around the camera. The camera moves
//! only when the player pushes past an edge, and only far enough to contain
//! them at that edge. It does not recenter.
//! Not simulation authority. Not AOI. Not a cinematic camera.

use purgatory_simulation::WorldBounds;

use crate::renderer::{Camera, clamp_camera_center};

/// Horizontal Dead Zone half-width (world units). Tiny taps stay inside this.
pub const DEAD_ZONE_HALF_X: f32 = 3.0;
/// Vertical Dead Zone half-height. Larger so normal jumps do not bob the camera.
pub const DEAD_ZONE_HALF_Y: f32 = 4.0;

/// Horizontal exponential smooth time (seconds to cover ~63% of remaining error).
pub const SMOOTH_TIME_X: f32 = 0.20;
/// Vertical smooth time. Slower than X so jump arcs are less camera-coupled.
pub const SMOOTH_TIME_Y: f32 = 0.40;

/// How the next camera step should treat a presentation commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CameraCommit {
    /// Map destination: snap to the dest local pose. Do not smooth across maps.
    SnapToPlayer,
    /// Channel/Instance: keep the current camera pose (same map, same local pose).
    PreservePose,
}

/// Tunable Dead Zone + damping. DEV starting values; not architecture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraFollowConfig {
    pub half_x: f32,
    pub half_y: f32,
    pub smooth_time_x: f32,
    pub smooth_time_y: f32,
}

impl Default for CameraFollowConfig {
    fn default() -> Self {
        Self {
            half_x: DEAD_ZONE_HALF_X,
            half_y: DEAD_ZONE_HALF_Y,
            smooth_time_x: SMOOTH_TIME_X,
            smooth_time_y: SMOOTH_TIME_Y,
        }
    }
}

/// Last computed follow intent (debug + next-step seed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraFollow {
    pub config: CameraFollowConfig,
    pub desired: [f32; 2],
    pub following_x: bool,
    pub following_y: bool,
}

impl Default for CameraFollow {
    fn default() -> Self {
        Self {
            config: CameraFollowConfig::default(),
            desired: [0.0, 0.0],
            following_x: false,
            following_y: false,
        }
    }
}

/// Shift `camera` only enough to keep `player` inside the Dead Zone on each axis.
///
/// Excess past an edge becomes the containment target. Inside the box the
/// target is the current camera (no motion, no center bias).
#[must_use]
pub fn contain_dead_zone(
    camera: [f32; 2],
    player: [f32; 2],
    half_x: f32,
    half_y: f32,
) -> ([f32; 2], bool, bool) {
    let mut desired = camera;
    let mut following_x = false;
    let mut following_y = false;
    let dx = player[0] - camera[0];
    if dx > half_x {
        desired[0] = player[0] - half_x;
        following_x = true;
    } else if dx < -half_x {
        desired[0] = player[0] + half_x;
        following_x = true;
    }
    let dy = player[1] - camera[1];
    if dy > half_y {
        desired[1] = player[1] - half_y;
        following_y = true;
    } else if dy < -half_y {
        desired[1] = player[1] + half_y;
        following_y = true;
    }
    (desired, following_x, following_y)
}

/// Frame-rate-independent exponential approach. Never overshoots `target`.
///
/// `out = current + (target - current) * (1 - exp(-dt / smooth_time))`
#[must_use]
pub fn damp(current: f32, target: f32, dt: f32, smooth_time: f32) -> f32 {
    if !current.is_finite() {
        return target;
    }
    if dt <= 0.0 || smooth_time <= 1e-6 {
        return target;
    }
    let alpha = 1.0 - (-dt / smooth_time).exp();
    current + (target - current) * alpha.clamp(0.0, 1.0)
}

impl CameraFollow {
    /// Seed smoothing from the displayed camera so a membership commit does not jump.
    pub fn seed_from(&mut self, position: [f32; 2]) {
        self.desired = position;
        self.following_x = false;
        self.following_y = false;
    }

    /// Instantly place the camera on `player` (clamped). Used for map destinations.
    pub fn snap(&mut self, camera: &mut Camera, player: [f32; 2], bounds: WorldBounds) {
        camera.follow_clamped(player, bounds);
        self.desired = camera.position;
        self.following_x = false;
        self.following_y = false;
    }

    /// Contain the player in the Dead Zone; damp toward that target. No recenter.
    pub fn step(&mut self, camera: &mut Camera, player: [f32; 2], bounds: WorldBounds, dt: f32) {
        let (desired_unclamped, following_x, following_y) = contain_dead_zone(
            camera.position,
            player,
            self.config.half_x,
            self.config.half_y,
        );
        self.following_x = following_x;
        self.following_y = following_y;
        let desired = clamp_camera_center(
            desired_unclamped,
            camera.viewport_width,
            camera.viewport_height,
            bounds,
        );
        self.desired = desired;
        let damped = [
            damp(
                camera.position[0],
                desired[0],
                dt,
                self.config.smooth_time_x,
            ),
            damp(
                camera.position[1],
                desired[1],
                dt,
                self.config.smooth_time_y,
            ),
        ];
        camera.position = clamp_camera_center(
            damped,
            camera.viewport_width,
            camera.viewport_height,
            bounds,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::Camera;

    #[test]
    fn dead_zone_matches_server_aoi_policy() {
        assert!(
            (DEAD_ZONE_HALF_X - purgatory_simulation::AOI_CAMERA_DEAD_ZONE_HALF[0]).abs()
                < f32::EPSILON
        );
        assert!(
            (DEAD_ZONE_HALF_Y - purgatory_simulation::AOI_CAMERA_DEAD_ZONE_HALF[1]).abs()
                < f32::EPSILON
        );
    }

    fn cam_at(position: [f32; 2]) -> Camera {
        Camera {
            position,
            viewport_width: 16.0,
            viewport_height: 9.0,
        }
    }

    fn wide_bounds() -> WorldBounds {
        WorldBounds {
            min_x: -40.0,
            max_x: 40.0,
            min_y: -20.0,
            max_y: 20.0,
        }
    }

    #[test]
    fn inside_horizontal_zone_leaves_target_unchanged() {
        let camera = [0.0, 0.0];
        let (desired, fx, fy) = contain_dead_zone(camera, [1.5, 0.0], 2.0, 4.0);
        assert_eq!(desired, camera);
        assert!(!fx);
        assert!(!fy);
    }

    #[test]
    fn crossing_right_edge_shifts_target_to_boundary() {
        let (desired, fx, fy) = contain_dead_zone([0.0, 0.0], [2.5, 0.0], 2.0, 4.0);
        assert!((desired[0] - 0.5).abs() < 1e-5, "got {}", desired[0]);
        assert!(fx);
        assert!(!fy);
        assert!((desired[1]).abs() < 1e-5);
    }

    #[test]
    fn crossing_left_edge_shifts_target_to_boundary() {
        let (desired, fx, _) = contain_dead_zone([0.0, 0.0], [-2.5, 0.0], 2.0, 4.0);
        assert!((desired[0] - (-0.5)).abs() < 1e-5, "got {}", desired[0]);
        assert!(fx);
    }

    #[test]
    fn return_inside_does_not_drift_target() {
        let camera = [1.0, 0.0];
        let (desired, fx, _) = contain_dead_zone(camera, [1.2, 0.0], 2.0, 4.0);
        assert_eq!(desired, camera);
        assert!(!fx);
    }

    #[test]
    fn contain_does_not_snap_desired_to_player() {
        let player = [5.0, 0.0];
        let (desired, fx, _) = contain_dead_zone([0.0, 0.0], player, 2.0, 4.0);
        assert!(fx);
        assert!(
            (desired[0] - player[0]).abs() > 1.0,
            "must keep player at the boundary, not recenter"
        );
        assert!((desired[0] - (player[0] - 2.0)).abs() < 1e-5);
    }

    #[test]
    fn vertical_zone_ignores_small_jumps() {
        let (desired, fx, fy) = contain_dead_zone([0.0, 0.0], [0.0, 3.0], 2.0, 4.0);
        assert_eq!(desired, [0.0, 0.0]);
        assert!(!fx);
        assert!(!fy);
    }

    #[test]
    fn damp_converges_without_overshoot() {
        let mut x = 0.0;
        let target = 10.0;
        let mut prev = x;
        for _ in 0..120 {
            x = damp(x, target, 1.0 / 60.0, 0.20);
            assert!(x <= target + 1e-4, "overshoot {x}");
            assert!(x >= prev - 1e-6, "must not reverse");
            prev = x;
        }
        assert!(
            (x - target).abs() < 0.05,
            "should be near target after 2s, got {x}"
        );
    }

    #[test]
    fn damp_is_approximately_frame_rate_independent() {
        fn advance(dt: f32, duration: f32) -> f32 {
            let mut x = 0.0;
            let mut t = 0.0;
            while t < duration {
                x = damp(x, 8.0, dt, 0.20);
                t += dt;
            }
            x
        }
        let a = advance(1.0 / 30.0, 0.5);
        let b = advance(1.0 / 144.0, 0.5);
        assert!(
            (a - b).abs() < 0.08,
            "30fps={a} 144fps={b} should stay close"
        );
    }

    #[test]
    fn step_stays_inside_map_bounds() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut cam = Camera::footnote_test_dev();
        let mut follow = CameraFollow::default();
        let half_w = cam.viewport_width * 0.5;
        let left = bounds.min_x + half_w;
        cam.position = [left, 0.0];
        follow.seed_from(cam.position);
        for _ in 0..90 {
            follow.step(&mut cam, [bounds.min_x + 0.5, 0.0], bounds, 1.0 / 60.0);
            assert!(cam.position[0] + 1e-3 >= left);
            assert!(cam.position[0] <= bounds.max_x - half_w + 1e-3);
        }
        assert!(
            (cam.position[0] - left).abs() < 0.05,
            "must settle at left clamp, got {}",
            cam.position[0]
        );
    }

    #[test]
    fn right_bound_settles_without_oscillation() {
        let bounds = WorldBounds::FOOTNOTE_TEST;
        let mut cam = Camera::footnote_test_dev();
        let mut follow = CameraFollow::default();
        let half_w = cam.viewport_width * 0.5;
        let right = bounds.max_x - half_w;
        cam.position = [right, 0.0];
        follow.seed_from(cam.position);
        let mut prev = cam.position[0];
        for _ in 0..90 {
            follow.step(&mut cam, [bounds.max_x - 0.4, 0.0], bounds, 1.0 / 60.0);
            assert!(cam.position[0] <= right + 1e-3);
            assert!(
                cam.position[0] <= prev + 0.05,
                "must not bounce outward at the right clamp"
            );
            prev = cam.position[0];
        }
        assert!((cam.position[0] - right).abs() < 0.05);
    }

    #[test]
    fn map_destination_snaps_without_smoothing() {
        let bounds = wide_bounds();
        let mut cam = cam_at([30.0, 8.0]);
        let mut follow = CameraFollow {
            desired: cam.position,
            ..CameraFollow::default()
        };
        follow.snap(&mut cam, [2.0, -1.0], bounds);
        assert!((cam.position[0] - 2.0).abs() < 1e-3);
        assert!((cam.position[1] - (-1.0)).abs() < 1e-3);
        assert_eq!(follow.desired, cam.position);
        assert!(!follow.following_x && !follow.following_y);
    }

    #[test]
    fn channel_preserve_does_not_jump() {
        let bounds = wide_bounds();
        let mut cam = cam_at([3.0, 1.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        follow.step(&mut cam, [3.4, 1.1], bounds, 1.0 / 60.0);
        assert!(
            (cam.position[0] - 3.0).abs() < 1e-4,
            "player still inside Dead Zone; camera must stay, got {:?}",
            cam.position
        );
        assert!((cam.position[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn steady_right_motion_enters_follow_once() {
        let bounds = wide_bounds();
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut x = 0.0;
        let mut flips = 0u32;
        let mut prev = false;
        for i in 0..180usize {
            x += 6.0 / 60.0;
            follow.step(&mut cam, [x, 0.0], bounds, 1.0 / 60.0);
            if i > 0 && follow.following_x != prev {
                flips += 1;
            }
            prev = follow.following_x;
        }
        assert!(
            flips <= 1,
            "monotonic motion must not chatter follow_x ({flips})"
        );
        assert!(follow.following_x);
    }

    #[test]
    fn inside_deadzone_leaves_camera_still() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut x = 0.0;
        while x < DEAD_ZONE_HALF_X - 0.5 {
            x += 6.0 * dt;
            follow.step(&mut cam, [x, 0.0], bounds, dt);
            assert!(
                cam.position[0].abs() < 1e-5,
                "camera must not move inside the box, x={x} cam={}",
                cam.position[0]
            );
            assert!(!follow.following_x);
        }
    }

    #[test]
    fn crossing_right_edge_starts_smooth_containment() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        follow.step(&mut cam, [DEAD_ZONE_HALF_X - 0.01, 0.0], bounds, dt);
        assert!((cam.position[0]).abs() < 1e-6);
        follow.step(&mut cam, [DEAD_ZONE_HALF_X + 0.05, 0.0], bounds, dt);
        assert!(follow.following_x);
        assert!(
            cam.position[0] > 0.0 && cam.position[0] < 0.03,
            "must start from excess only, got {}",
            cam.position[0]
        );
        assert!(
            (follow.desired[0] - 0.05).abs() < 1e-4,
            "target is right-edge containment, got {}",
            follow.desired[0]
        );
    }

    #[test]
    fn stop_settles_player_at_right_edge_not_center() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let player = DEAD_ZONE_HALF_X + 0.6;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        follow.step(&mut cam, [player, 0.0], bounds, dt);
        for _ in 0..240 {
            follow.step(&mut cam, [player, 0.0], bounds, dt);
        }
        let edge = player - DEAD_ZONE_HALF_X;
        assert!(
            (cam.position[0] - edge).abs() < 0.05,
            "must park the player on the right edge, camera {} want {edge}",
            cam.position[0]
        );
        assert!(
            (player - cam.position[0]).abs() > 1.5,
            "must not recenter; screen offset {}",
            player - cam.position[0]
        );
        let parked = cam.position[0];
        follow.step(&mut cam, [player, 0.0], bounds, dt);
        assert!(
            (cam.position[0] - parked).abs() < 1e-3,
            "once contained at the edge the camera must stay put"
        );
    }

    #[test]
    fn after_right_edge_leftward_crosses_box_without_camera_move() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let stop = DEAD_ZONE_HALF_X + 0.6;
        for _ in 0..240 {
            follow.step(&mut cam, [stop, 0.0], bounds, dt);
        }
        let parked = cam.position[0];
        let mut x = stop;
        while x > parked - DEAD_ZONE_HALF_X + 0.05 {
            x -= 6.0 * dt;
            follow.step(&mut cam, [x, 0.0], bounds, dt);
            assert!(
                (cam.position[0] - parked).abs() < 1e-4,
                "camera must stay while the player crosses the box, x={x} cam={}",
                cam.position[0]
            );
            assert!(!follow.following_x);
        }
    }

    #[test]
    fn crossing_left_edge_follows_left() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let stop = DEAD_ZONE_HALF_X + 0.6;
        for _ in 0..240 {
            follow.step(&mut cam, [stop, 0.0], bounds, dt);
        }
        let parked = cam.position[0];
        let mut x = stop;
        let left_edge = parked - DEAD_ZONE_HALF_X;
        while x > left_edge - 0.08 {
            x -= 6.0 * dt;
            follow.step(&mut cam, [x, 0.0], bounds, dt);
        }
        assert!(follow.following_x);
        assert!(
            cam.position[0] < parked - 1e-4,
            "must follow left past the left edge, parked {parked} now {}",
            cam.position[0]
        );
        assert!(
            follow.desired[0] < parked,
            "containment target must be left of the parked camera"
        );
    }

    #[test]
    fn activation_frame_targets_excess_not_player_center() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);

        let inside = [DEAD_ZONE_HALF_X - 0.01, 0.0];
        follow.step(&mut cam, inside, bounds, dt);
        assert!(!follow.following_x);
        assert!((cam.position[0]).abs() < 1e-6);
        let prev_cam = cam.position[0];
        let prev_desired = follow.desired[0];
        let prev_following = follow.following_x;

        let player = [DEAD_ZONE_HALF_X + 0.05, 0.0];
        let screen_before = player[0] - prev_cam;
        follow.step(&mut cam, player, bounds, dt);

        let excess = player[0] - DEAD_ZONE_HALF_X - prev_cam;
        let target_delta = follow.desired[0] - prev_desired;
        let camera_delta = cam.position[0] - prev_cam;
        let screen_after = player[0] - cam.position[0];

        assert!(!prev_following && follow.following_x);
        assert!(
            (follow.desired[0] - (player[0] - DEAD_ZONE_HALF_X)).abs() < 1e-4,
            "new target must be the deadzone edge, got {}",
            follow.desired[0]
        );
        assert!(
            (follow.desired[0] - player[0]).abs() > 1.0,
            "activation must not retarget player center"
        );
        assert!(
            (target_delta - excess).abs() < 1e-4,
            "target delta must equal excess ({excess}), got {target_delta}"
        );
        assert!(target_delta < 0.2 && camera_delta < 0.03);
        assert!(
            (screen_after - screen_before).abs() < 0.03,
            "screen-space player offset must stay continuous ({screen_before} → {screen_after})"
        );
    }

    #[test]
    fn slow_walk_across_edge_starts_from_near_zero_camera_velocity() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut x = 0.0;
        let mut prev_cam = 0.0;
        let mut activation_delta = None;
        let mut later_deltas = Vec::new();
        for i in 0..90usize {
            x += 6.0 * dt;
            follow.step(&mut cam, [x, 0.0], bounds, dt);
            let delta = (cam.position[0] - prev_cam).abs();
            if follow.following_x && activation_delta.is_none() {
                activation_delta = Some(delta);
            } else if activation_delta.is_some() && i < 40 {
                later_deltas.push(delta);
            }
            prev_cam = cam.position[0];
        }
        let first = activation_delta.expect("must cross the deadzone");
        assert!(
            first < 0.04,
            "activation camera delta must start near zero, got {first}"
        );
        let later_max = later_deltas.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            later_max > first,
            "already-following motion should exceed the activation kick ({later_max} vs {first})"
        );
    }

    #[test]
    fn reverse_immediately_after_follow_begins_is_continuous() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        follow.step(&mut cam, [DEAD_ZONE_HALF_X + 0.05, 0.0], bounds, dt);
        assert!(follow.following_x);
        let after_activate = cam.position[0];
        let prev_desired = follow.desired[0];
        follow.step(&mut cam, [DEAD_ZONE_HALF_X - 0.4, 0.0], bounds, dt);
        assert!(
            !follow.following_x,
            "player is back inside the box; camera must not keep following"
        );
        assert!(
            (follow.desired[0] - cam.position[0]).abs() < 1e-4,
            "inside the box the target is the current camera"
        );
        assert!(
            (cam.position[0] - after_activate).abs() < 0.03,
            "reverse must stay continuous, got {}",
            cam.position[0]
        );
        assert!((follow.desired[0] - prev_desired).abs() < 0.05);
    }

    #[test]
    fn repeated_edge_crossings_produce_no_jump() {
        let bounds = wide_bounds();
        let dt = 1.0 / 60.0;
        let mut cam = cam_at([0.0, 0.0]);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut x = 0.0;
        let mut vx = 6.0;
        let mut prev_cam = 0.0;
        let mut prev_desired = 0.0;
        let mut max_cam_delta = 0.0_f32;
        let mut max_desired_delta = 0.0_f32;
        for i in 0..360usize {
            x += vx * dt;
            if x > 3.5 {
                vx = -6.0;
            } else if x < -3.5 {
                vx = 6.0;
            }
            follow.step(&mut cam, [x, 0.0], bounds, dt);
            if i > 0 {
                max_cam_delta = max_cam_delta.max((cam.position[0] - prev_cam).abs());
                max_desired_delta = max_desired_delta.max((follow.desired[0] - prev_desired).abs());
            }
            prev_cam = cam.position[0];
            prev_desired = follow.desired[0];
        }
        assert!(
            max_desired_delta < 0.25,
            "containment target must not jump toward center, got {max_desired_delta}"
        );
        assert!(
            max_cam_delta < 0.15,
            "camera must not kick on repeated edge crossings, got {max_cam_delta}"
        );
    }

    #[test]
    fn camera_commit_kinds_are_distinct() {
        assert_ne!(CameraCommit::SnapToPlayer, CameraCommit::PreservePose);
    }
}
