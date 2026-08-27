//! Presentation-only camera discontinuity tracking.
//!
//! Separates camera jumps from simulation player teleports.

/// Last-frame camera follow diagnostics for the debug overlay.
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraMotionDebug {
    pub previous_position: [f32; 2],
    pub delta: [f32; 2],
    pub discontinuity: bool,
    /// Why the camera center may differ from the raw follow target.
    pub clamp_reason: CameraClampReason,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CameraClampReason {
    #[default]
    None,
    Follow,
    /// Follow target was moved to keep the viewport inside world bounds.
    ClampedToBounds,
    /// Follow disabled; only bounds clamp applied.
    BoundsOnly,
}

impl CameraMotionDebug {
    /// Update from a finished camera step. `raw_follow` is the unclamped center
    /// request (usually player position) when following; `None` when not following.
    #[must_use]
    pub fn record(
        previous: [f32; 2],
        position: [f32; 2],
        raw_follow: Option<[f32; 2]>,
        follow_enabled: bool,
    ) -> Self {
        let delta = [position[0] - previous[0], position[1] - previous[1]];
        let delta_len = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
        const MAX_STEP: f32 = 1.2;
        let discontinuity = delta_len > MAX_STEP;
        let _ = delta_len;
        let clamp_reason = if !follow_enabled {
            CameraClampReason::BoundsOnly
        } else if let Some(raw) = raw_follow {
            let clamped =
                (position[0] - raw[0]).abs() > 1e-4 || (position[1] - raw[1]).abs() > 1e-4;
            if clamped {
                CameraClampReason::ClampedToBounds
            } else {
                CameraClampReason::Follow
            }
        } else {
            CameraClampReason::None
        };
        if discontinuity {
            // Logging is gated by the client debug overlay toggles.
        }
        Self {
            previous_position: previous,
            delta,
            discontinuity,
            clamp_reason,
        }
    }
}
