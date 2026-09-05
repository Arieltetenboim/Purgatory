//! Authoritative presentation one-shot (Attack/Hurt) semantic state.
//!
//! Duration and interruption are simulation-owned. Clip completion must never
//! clear or grant this state. Not combat hit detection.

use crate::time::SimulationTick;

/// Authoritative one-shot kinds visible to presentation. Not animation frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationOneShotKind {
    Attack,
    Hurt,
}

impl PresentationOneShotKind {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Attack => 1,
            Self::Hurt => 2,
        }
    }

    #[must_use]
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Attack),
            2 => Some(Self::Hurt),
            _ => None,
        }
    }
}

/// Live one-shot on an entity until `until_tick` (exclusive clear at tick >=).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationOneShot {
    pub kind: PresentationOneShotKind,
    pub until_tick: SimulationTick,
}

/// Why a oneshot start was rejected. Animation is not consulted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationOneShotError {
    /// Attack while Hurt is active (A5 v0: Attack does not interrupt Hurt).
    BlockedByHurt,
}

/// Attack / Hurt duration in simulation ticks (30 Hz). Slightly longer than the
/// debug clip so presentation can hold the final pose while semantic state remains.
pub const ATTACK_DURATION_TICKS: u64 = 18; // 0.60 s (> 0.40 s clip)
pub const HURT_DURATION_TICKS: u64 = 17; // 0.566… s (> 0.35 s clip)

#[must_use]
pub const fn duration_ticks(kind: PresentationOneShotKind) -> u64 {
    match kind {
        PresentationOneShotKind::Attack => ATTACK_DURATION_TICKS,
        PresentationOneShotKind::Hurt => HURT_DURATION_TICKS,
    }
}

/// Apply A5 interruption policy without reading animation clocks.
///
/// ```text
/// Hurt may interrupt Attack
/// Attack does not interrupt Hurt
/// same-kind re-request refreshes until_tick (authoritative re-trigger)
/// ```
pub fn try_start_oneshot(
    current: Option<PresentationOneShot>,
    kind: PresentationOneShotKind,
    now: SimulationTick,
) -> Result<PresentationOneShot, PresentationOneShotError> {
    let active = current.filter(|o| now.get() < o.until_tick.get());
    match (active.map(|o| o.kind), kind) {
        (Some(PresentationOneShotKind::Hurt), PresentationOneShotKind::Attack) => {
            Err(PresentationOneShotError::BlockedByHurt)
        }
        _ => Ok(PresentationOneShot {
            kind,
            until_tick: now.saturating_add_ticks(duration_ticks(kind)),
        }),
    }
}

/// Clear when `now >= until_tick`. Does not consult clip completion.
#[must_use]
pub fn oneshot_if_active(
    oneshot: Option<PresentationOneShot>,
    now: SimulationTick,
) -> Option<PresentationOneShot> {
    oneshot.filter(|o| now.get() < o.until_tick.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hurt_interrupts_attack() {
        let now = SimulationTick::from_count(10);
        let attack = try_start_oneshot(None, PresentationOneShotKind::Attack, now).unwrap();
        let hurt = try_start_oneshot(Some(attack), PresentationOneShotKind::Hurt, now).unwrap();
        assert_eq!(hurt.kind, PresentationOneShotKind::Hurt);
    }

    #[test]
    fn attack_does_not_interrupt_hurt() {
        let now = SimulationTick::from_count(5);
        let hurt = try_start_oneshot(None, PresentationOneShotKind::Hurt, now).unwrap();
        assert_eq!(
            try_start_oneshot(Some(hurt), PresentationOneShotKind::Attack, now),
            Err(PresentationOneShotError::BlockedByHurt)
        );
    }

    #[test]
    fn expired_oneshot_does_not_block() {
        let started = SimulationTick::from_count(1);
        let attack = try_start_oneshot(None, PresentationOneShotKind::Attack, started).unwrap();
        let later = attack.until_tick;
        assert!(oneshot_if_active(Some(attack), later).is_none());
        assert!(try_start_oneshot(Some(attack), PresentationOneShotKind::Attack, later).is_ok());
    }
}
