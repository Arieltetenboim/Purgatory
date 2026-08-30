//! Local-player visual pose: correction smoothing between prediction and draw.
//!
//! Does **not** feed simulation, commands, reconciliation, or AOI.
//! Does **not** use the remote interpolation buffer.

use std::time::Duration;

use purgatory_simulation::TICK_DURATION;

use crate::camera_follow::damp;

/// Seconds to cover ~63% of a remaining visual offset.
pub const PRESENTATION_SMOOTH_TIME: f32 = 0.08;

/// World-unit offset (or single correction) at or above this snaps, not damps.
pub const PRESENTATION_SNAP_DISTANCE: f32 = 1.0;

/// Ignore remainder extra at or below this speed (wu/s). Walking (6) still extras.
pub const EXTRAPOLATE_MIN_SPEED: f32 = 0.5;

/// Per-frame local poses shared by camera follow and local-player draw.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameLocalPose {
    pub predicted: Option<[f32; 2]>,
    pub presented: Option<[f32; 2]>,
    pub replica: Option<[f32; 2]>,
    pub correction_delta: [f32; 2],
    pub reconciled: bool,
}

/// Decaying visual offset so small restore+replay pops do not flicker on screen.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalPresentation {
    offset: [f32; 2],
    pose: Option<[f32; 2]>,
    snap_pending: bool,
}

impl LocalPresentation {
    #[must_use]
    pub fn pose(&self) -> Option<[f32; 2]> {
        self.pose
    }

    #[must_use]
    pub fn offset(&self) -> [f32; 2] {
        self.offset
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Next [`Self::step`] copies predicted pose with zero offset.
    pub fn request_snap(&mut self) {
        self.snap_pending = true;
    }

    /// Predicted pose jumped by `delta` (`post - pre`). Keep the visual pose still.
    pub fn absorb_correction(&mut self, delta: [f32; 2]) {
        if !delta[0].is_finite() || !delta[1].is_finite() {
            self.request_snap();
            return;
        }
        self.offset[0] -= delta[0];
        self.offset[1] -= delta[1];
    }

    /// `presented = predicted + decaying offset`. Call **once** per render frame.
    pub fn step(&mut self, predicted: Option<[f32; 2]>, dt: f32) -> Option<[f32; 2]> {
        let Some(target) = predicted else {
            self.clear();
            return None;
        };
        if !target[0].is_finite() || !target[1].is_finite() {
            self.clear();
            return None;
        }
        if self.snap_pending || self.pose.is_none() {
            self.offset = [0.0, 0.0];
            self.pose = Some(target);
            self.snap_pending = false;
            return self.pose;
        }
        if offset_length(self.offset) >= PRESENTATION_SNAP_DISTANCE {
            self.offset = [0.0, 0.0];
            self.pose = Some(target);
            return self.pose;
        }
        self.offset[0] = damp(self.offset[0], 0.0, dt, PRESENTATION_SMOOTH_TIME);
        self.offset[1] = damp(self.offset[1], 0.0, dt, PRESENTATION_SMOOTH_TIME);
        self.pose = Some([target[0] + self.offset[0], target[1] + self.offset[1]]);
        self.pose
    }
}

/// Advance last-tick pose by leftover accumulator time using last-tick velocity.
///
/// Remainder `0` shows the tick pose immediately (no added delay). Does not
/// mutate simulation. Clamps remainder to one tick so hitch leftovers cannot
/// overshoot a full extra step.
///
/// Speeds below [`EXTRAPOLATE_MIN_SPEED`] are treated as idle: leftover replica
/// velocity after the last moving Update must not sawtooth the sprite against
/// `clock.remainder()` while the tick pose is stable.
#[must_use]
pub fn extrapolate_tick_pose(pose: [f32; 2], velocity: [f32; 2], remainder: Duration) -> [f32; 2] {
    let speed_sq = velocity[0] * velocity[0] + velocity[1] * velocity[1];
    if speed_sq < EXTRAPOLATE_MIN_SPEED * EXTRAPOLATE_MIN_SPEED {
        return pose;
    }
    let mut t = remainder.as_secs_f32();
    if !t.is_finite() || t <= 0.0 {
        return pose;
    }
    let tick = TICK_DURATION.as_secs_f32();
    if t > tick {
        t = tick;
    }
    let dx = velocity[0] * t;
    let dy = velocity[1] * t;
    if !dx.is_finite() || !dy.is_finite() {
        return pose;
    }
    [pose[0] + dx, pose[1] + dy]
}

#[must_use]
pub fn offset_length(offset: [f32; 2]) -> f32 {
    (offset[0] * offset[0] + offset[1] * offset[1]).sqrt()
}

#[must_use]
pub fn screen_space_x(world_x: f32, camera_x: f32) -> f32 {
    world_x - camera_x
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::camera_follow::{CameraFollow, DEAD_ZONE_HALF_X};
    use crate::renderer::Camera;
    use purgatory_simulation::{TICK_DURATION, WorldBounds};

    fn wide_bounds() -> WorldBounds {
        WorldBounds {
            min_x: -80.0,
            max_x: 80.0,
            min_y: -20.0,
            max_y: 20.0,
        }
    }

    fn cam_at(x: f32) -> Camera {
        Camera {
            position: [x, 0.0],
            viewport_width: 16.0,
            viewport_height: 9.0,
        }
    }

    fn max_abs_delta(xs: &[f32]) -> f32 {
        xs.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max)
    }

    /// Raw predicted X + damped containment follow: corrections still leak to screen.
    #[test]
    fn damped_camera_exposes_raw_prediction_corrections() {
        let mut cam = cam_at(0.0);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let vx = 6.0;
        let dt = 1.0 / 60.0;
        let mut pred_x = DEAD_ZONE_HALF_X + 0.05;
        let mut pred_xs = Vec::new();
        let mut cam_xs = Vec::new();
        let mut screen_xs = Vec::new();
        for i in 0..180usize {
            pred_x += vx * dt;
            if i.is_multiple_of(4) {
                pred_x += if (i / 4).is_multiple_of(2) {
                    0.08
                } else {
                    -0.08
                };
            }
            follow.step(&mut cam, [pred_x, 0.0], wide_bounds(), dt);
            pred_xs.push(pred_x);
            cam_xs.push(cam.position[0]);
            screen_xs.push(screen_space_x(pred_x, cam.position[0]));
        }
        let pred_jump = max_abs_delta(&pred_xs);
        let cam_jump = max_abs_delta(&cam_xs);
        let screen_jump = max_abs_delta(&screen_xs);
        assert!(
            pred_jump > 0.07,
            "world-space predicted X must itself jump, got {pred_jump}"
        );
        assert!(
            cam_jump > 0.02,
            "containment follow must move the camera (got {cam_jump})"
        );
        assert!(
            screen_jump > 0.05,
            "screen-space X must show the correction (got {screen_jump})"
        );
    }

    #[test]
    fn presentation_smoothing_hides_small_corrections_in_screen_space() {
        let mut cam = cam_at(0.0);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut pres = LocalPresentation::default();
        let vx = 6.0;
        let dt = 1.0 / 60.0;
        let mut pred_x = DEAD_ZONE_HALF_X;
        let _ = pres.step(Some([pred_x, 0.0]), dt);
        follow.step(&mut cam, [pred_x, 0.0], wide_bounds(), dt);
        let mut pred_xs = Vec::new();
        let mut cam_xs = Vec::new();
        let mut screen_xs = Vec::new();
        let mut follow_flips = 0u32;
        let mut prev_follow = follow.following_x;
        for i in 0..180usize {
            pred_x += vx * dt;
            if i.is_multiple_of(4) {
                let delta = if (i / 4).is_multiple_of(2) {
                    0.08
                } else {
                    -0.08
                };
                pred_x += delta;
                pres.absorb_correction([delta, 0.0]);
            }
            let presented = pres.step(Some([pred_x, 0.0]), dt).unwrap();
            follow.step(&mut cam, presented, wide_bounds(), dt);
            if i >= 20 {
                pred_xs.push(pred_x);
                cam_xs.push(cam.position[0]);
                screen_xs.push(screen_space_x(presented[0], cam.position[0]));
                if follow.following_x != prev_follow {
                    follow_flips += 1;
                }
            }
            prev_follow = follow.following_x;
        }
        assert!(max_abs_delta(&pred_xs) > 0.07);
        let screen_jump = max_abs_delta(&screen_xs);
        assert!(
            screen_jump < 0.025,
            "presented+camera screen X must stay smooth, got {screen_jump}"
        );
        assert!(
            max_abs_delta(&cam_xs) < 0.25,
            "camera itself must remain smooth"
        );
        assert!(
            follow_flips <= 2,
            "steady motion past the edge must not chatter follow ({follow_flips})"
        );
        assert!(
            offset_length(pres.offset()) < 0.12,
            "offset must not accumulate"
        );
    }

    #[test]
    fn large_correction_snaps() {
        let mut pres = LocalPresentation::default();
        let _ = pres.step(Some([0.0, 0.0]), 1.0 / 60.0);
        pres.absorb_correction([1.5, 0.0]);
        let pose = pres.step(Some([1.5, 0.0]), 1.0 / 60.0).unwrap();
        assert!(
            (pose[0] - 1.5).abs() < 1e-5,
            "must snap to predicted, got {}",
            pose[0]
        );
        assert_eq!(pres.offset(), [0.0, 0.0]);
    }

    #[test]
    fn request_snap_clears_offset_on_discontinuity() {
        let mut pres = LocalPresentation::default();
        let _ = pres.step(Some([2.0, 1.0]), 1.0 / 60.0);
        pres.absorb_correction([0.2, 0.0]);
        pres.request_snap();
        let pose = pres.step(Some([10.0, -3.0]), 1.0 / 60.0).unwrap();
        assert_eq!(pose, [10.0, -3.0]);
        assert_eq!(pres.offset(), [0.0, 0.0]);
    }

    #[test]
    fn prediction_tick_is_applied_the_same_frame() {
        let mut pres = LocalPresentation::default();
        let _ = pres.step(Some([0.0, 0.0]), 1.0 / 60.0);
        let pose = pres.step(Some([0.20, 0.0]), 1.0 / 60.0).unwrap();
        assert!(
            (pose[0] - 0.20).abs() < 1e-4,
            "no leftover offset: controls must not lag, got {}",
            pose[0]
        );
    }

    #[test]
    fn camera_and_render_share_finalized_pose() {
        let mut pres = LocalPresentation::default();
        let mut cam = cam_at(0.0);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        pres.absorb_correction([0.05, 0.0]);
        let presented = pres.step(Some([2.4, 0.0]), 1.0 / 60.0).unwrap();
        follow.step(&mut cam, presented, wide_bounds(), 1.0 / 60.0);
        assert_eq!(presented, pres.pose().unwrap());
        let frame = FrameLocalPose {
            predicted: Some([2.4, 0.0]),
            presented: Some(presented),
            replica: Some([2.35, 0.0]),
            correction_delta: [0.05, 0.0],
            reconciled: true,
        };
        assert_eq!(frame.presented, Some(presented));
        assert_eq!(pres.pose(), frame.presented);
    }

    #[test]
    fn inside_dead_zone_camera_still_with_smoothed_pose() {
        let mut cam = cam_at(0.0);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let mut pres = LocalPresentation::default();
        for _ in 0..30 {
            let presented = pres.step(Some([1.2, 0.0]), 1.0 / 60.0).unwrap();
            follow.step(&mut cam, presented, wide_bounds(), 1.0 / 60.0);
        }
        assert!((cam.position[0]).abs() < 1e-4);
        assert!(!follow.following_x);
    }

    #[test]
    fn offset_decays_to_prediction() {
        let mut pres = LocalPresentation::default();
        let _ = pres.step(Some([0.0, 0.0]), 1.0 / 60.0);
        pres.absorb_correction([0.3, 0.0]);
        let mut last = [0.0, 0.0];
        for _ in 0..120 {
            last = pres.step(Some([0.3, 0.0]), 1.0 / 60.0).unwrap();
        }
        assert!((last[0] - 0.3).abs() < 0.02);
        assert!(offset_length(pres.offset()) < 0.02);
    }

    #[test]
    fn remainder_zero_matches_tick_pose() {
        let pose = [3.0, -1.0];
        let out = extrapolate_tick_pose(pose, [6.0, 2.0], Duration::ZERO);
        assert_eq!(out, pose);
    }

    #[test]
    fn leftover_idle_velocity_does_not_extrapolate() {
        let pose = [4.0, 1.0];
        let rem = Duration::from_secs_f32(TICK_DURATION.as_secs_f32() * 0.8);
        let out = extrapolate_tick_pose(pose, [0.3, 0.0], rem);
        assert_eq!(out, pose);
        let still = extrapolate_tick_pose(pose, [0.0, -0.2], rem);
        assert_eq!(still, pose);
    }

    #[test]
    fn remainder_extrapolation_advances_at_velocity() {
        let pose = [1.0, 0.0];
        let vel = [6.0, 0.0];
        let rem = Duration::from_secs_f32(1.0 / 60.0);
        let out = extrapolate_tick_pose(pose, vel, rem);
        assert!((out[0] - 1.1).abs() < 1e-5, "got {}", out[0]);
        assert!((out[1]).abs() < 1e-6);
    }

    #[test]
    fn remainder_wrap_stays_continuous_at_render_dt() {
        let vel = [6.0, 0.0];
        let dt = 1.0 / 144.0;
        let tick = TICK_DURATION.as_secs_f32();
        let rem_before = tick - dt * 0.4;
        let pose_old = [2.0, 0.0];
        let before = extrapolate_tick_pose(pose_old, vel, Duration::from_secs_f32(rem_before));
        let pose_new = [pose_old[0] + vel[0] * tick, 0.0];
        let rem_after = rem_before + dt - tick;
        let after = extrapolate_tick_pose(pose_new, vel, Duration::from_secs_f32(rem_after));
        let step = after[0] - before[0];
        assert!(
            (step - vel[0] * dt).abs() < 1e-4,
            "tick wrap must equal vel*dt, got {step} want {}",
            vel[0] * dt
        );
    }

    #[test]
    fn remainder_extrapolation_hides_30hz_steps_from_damped_camera() {
        let mut cam = cam_at(2.0);
        let mut follow = CameraFollow::default();
        follow.seed_from(cam.position);
        let vx = 6.0;
        let vel = [vx, 0.0];
        let mut pred_x = 4.0;
        let dt = 1.0 / 60.0;
        let mut screen_xs = Vec::new();
        let mut presented_xs = Vec::new();
        let mut idle_d = 0.0_f32;
        let mut tick_d = 0.0_f32;
        let mut prev_p = pred_x;
        for i in 0..120usize {
            let ticked = i.is_multiple_of(2);
            let rem = if ticked {
                pred_x += vx / 30.0;
                Duration::ZERO
            } else {
                Duration::from_secs_f32(dt)
            };
            let presented = extrapolate_tick_pose([pred_x, 0.0], vel, rem);
            follow.step(&mut cam, presented, wide_bounds(), dt);
            if i >= 4 {
                let dp = (presented[0] - prev_p).abs();
                if ticked {
                    tick_d = tick_d.max(dp);
                } else {
                    idle_d = idle_d.max(dp);
                }
            }
            if i >= 20 {
                presented_xs.push(presented[0]);
                screen_xs.push(screen_space_x(presented[0], cam.position[0]));
            }
            prev_p = presented[0];
        }
        assert!(idle_d > 0.05, "idle frames must advance, got {idle_d}");
        assert!(
            (tick_d - idle_d).abs() < 0.08,
            "tick vs idle presented steps must match, tick {tick_d} idle {idle_d}"
        );
        let screen_jump = max_abs_delta(&screen_xs);
        assert!(
            screen_jump < 0.06,
            "screen X must not sawtooth after remainder extra, got {screen_jump}"
        );
        assert!(max_abs_delta(&presented_xs) < 0.15);
    }
}
