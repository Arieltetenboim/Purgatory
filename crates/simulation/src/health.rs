//! Minimal life container. Not combat resolution.

/// Default max Health when a player is attached as a combatant.
pub const PLAYER_HEALTH_MAX: f32 = 20.0;

/// Fixed-timestep duration of normal victim damage immunity.
///
/// The 30 Hz simulation uses 61 ticks so the normal gate remains closed for
/// every tick before two seconds have elapsed.
pub const DAMAGE_IMMUNITY_TICKS: u64 = 61;

/// Whether authoritative damage should respect the victim's normal immunity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DamageImmunityPolicy {
    Respect,
    Bypass,
}

/// Current / max health. Optional capability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    #[must_use]
    pub const fn full(max: f32) -> Self {
        Self { current: max, max }
    }

    /// Authoritative post-respawn current HP: half of max, clamped to `[0, max]`.
    #[must_use]
    pub fn respawn_current(max: f32) -> f32 {
        (max * 0.5).clamp(0.0, max)
    }

    /// Restore a dead combatant to respawn HP without changing max.
    #[must_use]
    pub fn after_respawn(self) -> Self {
        Self {
            current: Self::respawn_current(self.max),
            max: self.max,
        }
    }

    /// Combat-alive. Entities without Health are non-participants, not dead.
    #[must_use]
    pub fn is_alive(self) -> bool {
        self.current > 0.0
    }

    #[must_use]
    pub fn is_dead(self) -> bool {
        !self.is_alive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_matches_max() {
        let h = Health::full(10.0);
        assert_eq!(h.current, 10.0);
        assert_eq!(h.max, 10.0);
        assert!(h.is_alive());
        assert!(!h.is_dead());
        let dead = Health {
            current: 0.0,
            max: 10.0,
        };
        assert!(dead.is_dead());
    }

    #[test]
    fn respawn_current_is_half_max_clamped() {
        assert_eq!(Health::respawn_current(20.0), 10.0);
        let restored = Health {
            current: 0.0,
            max: 21.0,
        }
        .after_respawn();
        assert_eq!(restored.current, 10.5);
        assert_eq!(restored.max, 21.0);
    }
}
