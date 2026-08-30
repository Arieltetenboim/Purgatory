//! Interactable capability. Type/intent marker only — not dialog, shop, or inventory.

/// Kind of interaction advertised by a runtime entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractableKind {
    Generic,
    Npc,
    Portal,
    Chest,
    Switch,
}

/// Optional capability: this entity may be a server-validated interaction target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Interactable {
    pub kind: InteractableKind,
}

impl Interactable {
    #[must_use]
    pub const fn new(kind: InteractableKind) -> Self {
        Self { kind }
    }
}

/// Authoritative interaction range in world units. Client nearest-target is advisory.
pub const INTERACT_RANGE: f32 = 2.5;

/// Half-extents of the portal activation AABB (world units). Smaller than
/// [`INTERACT_RANGE`]. The player center must lie inside this box.
pub const PORTAL_ACTIVATE_HALF: [f32; 2] = [0.5, 0.8];

/// Authoritative "approximately centered" check. Nearest-portal is not enough.
#[must_use]
pub fn in_portal_activation_zone(actor: [f32; 2], portal: [f32; 2]) -> bool {
    (actor[0] - portal[0]).abs() <= PORTAL_ACTIVATE_HALF[0]
        && (actor[1] - portal[1]).abs() <= PORTAL_ACTIVATE_HALF[1]
}

impl std::fmt::Display for InteractableKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Generic => "Generic",
            Self::Npc => "Npc",
            Self::Portal => "Portal",
            Self::Chest => "Chest",
            Self::Switch => "Switch",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_is_a_marker() {
        assert_ne!(InteractableKind::Npc, InteractableKind::Chest);
        assert_eq!(
            Interactable::new(InteractableKind::Portal).kind,
            InteractableKind::Portal
        );
    }

    #[test]
    fn activation_zone_is_smaller_than_interact_range() {
        const { assert!(PORTAL_ACTIVATE_HALF[0] < INTERACT_RANGE) };
        assert!(in_portal_activation_zone([6.0, -3.0], [6.0, -2.9]));
        assert!(!in_portal_activation_zone([4.4, -3.0], [6.0, -2.9]));
    }

    #[test]
    fn standing_on_map_a_portal_is_in_zone_and_chest_is_the_13_wu_generic() {
        let portal = [6.0, -2.9];
        let player = [6.0, -3.0];
        let chest = [-7.4, -2.9];
        assert!(in_portal_activation_zone(player, portal));
        let dx = player[0] - chest[0];
        let dy = player[1] - chest[1];
        let chest_dist = (dx * dx + dy * dy).sqrt();
        assert!(
            (chest_dist - 13.4).abs() < 0.15,
            "overlay ~13.3 is player-to-chest while standing on the portal, not a zone mismatch; got {chest_dist}"
        );
        assert!(chest_dist > INTERACT_RANGE);
    }
}
