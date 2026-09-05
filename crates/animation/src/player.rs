//! Mutable playback state above the explicit-time sampler.
//!
//! `advance` owns scaled dt (`dt × speed`). Once/Loop wrap is a read-only
//! projection in `sample_time`. `sample` (crate root) remains policy-free.

use crate::clip::{AnimationClip, LoopPolicy};

/// Why playback mutation was rejected. On error, player state is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerError {
    NonFiniteDt,
    NegativeDt,
    NonFiniteSpeed,
    NegativeSpeed,
}

/// Per-instance playback clock. Does not own clip data or mutate poses.
#[derive(Clone, Debug)]
pub struct AnimationPlayer {
    /// Accumulated playback time (implementation-owned; not a public contract).
    elapsed: f64,
    playing: bool,
    speed: f32,
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationPlayer {
    /// Starts paused at elapsed 0 with speed 1.0. No autoplay.
    #[must_use]
    pub fn new() -> Self {
        Self {
            elapsed: 0.0,
            playing: false,
            speed: 1.0,
        }
    }

    #[must_use]
    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    #[must_use]
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Changes speed only. Does not change elapsed or playing.
    pub fn set_speed(&mut self, speed: f32) -> Result<(), PlayerError> {
        if !speed.is_finite() {
            return Err(PlayerError::NonFiniteSpeed);
        }
        if speed < 0.0 {
            return Err(PlayerError::NegativeSpeed);
        }
        self.speed = speed;
        Ok(())
    }

    /// Reset elapsed to 0. Keeps playing and speed.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    /// Advance playback by caller-supplied frame `dt`.
    ///
    /// Applies `scaled_dt = dt × speed` internally. Do not pre-multiply speed.
    /// Paused: no-op `Ok`. Rejects non-finite / negative `dt` without mutation.
    /// `speed == 0` is valid (elapsed unchanged while playing).
    pub fn advance(&mut self, dt: f32, _clip: &AnimationClip) -> Result<(), PlayerError> {
        if !dt.is_finite() {
            return Err(PlayerError::NonFiniteDt);
        }
        if dt < 0.0 {
            return Err(PlayerError::NegativeDt);
        }
        if !self.playing {
            return Ok(());
        }
        self.elapsed += f64::from(dt) * f64::from(self.speed);
        Ok(())
    }

    /// Read-only clip-local sample time. No side effects.
    #[must_use]
    pub fn sample_time(&self, clip: &AnimationClip) -> f32 {
        let duration = f64::from(clip.duration());
        if !(duration.is_finite() && duration > 0.0) {
            return 0.0;
        }
        let t = match clip.loop_policy() {
            LoopPolicy::Once => self.elapsed.min(duration),
            LoopPolicy::Loop => {
                // rem_euclid: exact multiples of duration → 0 (including elapsed == duration).
                self.elapsed.rem_euclid(duration)
            }
        };
        t as f32
    }
}
