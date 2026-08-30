//! Transition gameplay input barrier (ADR-0042).
//!
//! Phase 6F absorbs [`InputGateReason`] as
//! [`crate::ActionDenialReason::TransitionLocked`]. This type still owns the
//! remaining-tick lock duration. Duration matches the presentation FadeOut +
//! minimum Hold contract (ADR-0041 / ADR-0042): the earliest honest FadeIn.
//! Client unlock is still state-driven (`FadeIn` begins).

use crate::time::TICK_DURATION;

/// Map FadeOut + min Hold. Keep in sync with client `MAP_FADE_OUT_SEC` +
/// `MAP_FADE_HOLD_SEC`.
pub const MAP_TRANSITION_INPUT_LOCK_SECS: f32 = 0.375;
/// Membership FadeOut + min Hold. Keep in sync with client
/// `MEMBERSHIP_FADE_OUT_SEC` + `MEMBERSHIP_FADE_HOLD_SEC`.
pub const MEMBERSHIP_TRANSITION_INPUT_LOCK_SECS: f32 = 0.250;

/// Why gameplay input is currently barred. Mapped to
/// [`crate::ActionDenialReason::TransitionLocked`] by the 6F action gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputGateReason {
    MapTransition,
    MembershipTransition,
}

impl InputGateReason {
    #[must_use]
    pub fn lock_ticks(self) -> u16 {
        match self {
            Self::MapTransition => map_transition_input_lock_ticks(),
            Self::MembershipTransition => membership_transition_input_lock_ticks(),
        }
    }

    #[must_use]
    pub fn debug_label(self) -> &'static str {
        match self {
            Self::MapTransition => "MAP TRANSITION",
            Self::MembershipTransition => "CHANNEL TRANSITION",
        }
    }
}

#[must_use]
pub fn map_transition_input_lock_ticks() -> u16 {
    lock_ticks_for(MAP_TRANSITION_INPUT_LOCK_SECS)
}

#[must_use]
pub fn membership_transition_input_lock_ticks() -> u16 {
    lock_ticks_for(MEMBERSHIP_TRANSITION_INPUT_LOCK_SECS)
}

#[must_use]
fn lock_ticks_for(secs: f32) -> u16 {
    let tick = TICK_DURATION.as_secs_f32();
    debug_assert!(tick > 0.0);
    let ticks = (secs / tick).ceil();
    if ticks < 1.0 { 1 } else { ticks as u16 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_lock_covers_fade_out_plus_hold() {
        let ticks = map_transition_input_lock_ticks();
        let covered = f32::from(ticks) * TICK_DURATION.as_secs_f32();
        assert!(
            covered + 1e-4 >= MAP_TRANSITION_INPUT_LOCK_SECS,
            "lock {ticks} ticks = {covered}s must cover {MAP_TRANSITION_INPUT_LOCK_SECS}s"
        );
        assert!(ticks >= 1);
    }

    #[test]
    fn membership_lock_covers_fade_out_plus_hold() {
        let ticks = membership_transition_input_lock_ticks();
        let covered = f32::from(ticks) * TICK_DURATION.as_secs_f32();
        assert!(covered + 1e-4 >= MEMBERSHIP_TRANSITION_INPUT_LOCK_SECS);
        assert!(ticks >= 1);
        assert!(ticks < map_transition_input_lock_ticks());
    }

    #[test]
    fn reason_ticks_match_helpers() {
        assert_eq!(
            InputGateReason::MapTransition.lock_ticks(),
            map_transition_input_lock_ticks()
        );
        assert_eq!(
            InputGateReason::MembershipTransition.lock_ticks(),
            membership_transition_input_lock_ticks()
        );
    }
}
