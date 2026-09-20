//! Server-only authored monster definitions.

use crate::error::{ContentError, ValidationIssue};
use purgatory_common::{ContentId, validate_authored_id};

/// Monster content schema v4.
pub const MONSTER_CONTENT_SCHEMA_VERSION: u32 = 4;

/// The intentionally small behavior vocabulary supported by Monster schema v4.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonsterBehavior {
    /// Patrol until damaged by a player, then pursue that attacker and deal contact damage.
    ChaseContactWhenAttacked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonsterCollisionBounds {
    /// Distance from entity origin to the left collision edge.
    pub left: f32,
    /// Distance from entity origin to the right collision edge.
    pub right: f32,
    /// Distance from entity origin to the bottom collision edge.
    pub bottom: f32,
    /// Distance from entity origin to the top collision edge.
    pub top: f32,
}

impl MonsterCollisionBounds {
    #[must_use]
    pub fn half_extents(self) -> [f32; 2] {
        [
            (self.left + self.right) * 0.5,
            (self.bottom + self.top) * 0.5,
        ]
    }

    #[must_use]
    pub fn center_offset(self) -> [f32; 2] {
        [
            (self.right - self.left) * 0.5,
            (self.top - self.bottom) * 0.5,
        ]
    }
}

/// Client-safe presentation projection from the same authored Monster JSON.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterPresentationDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub sprite_id: String,
}

pub fn validate_monster_presentation(
    def: &MonsterPresentationDefinition,
) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(err) = validate_authored_id(&def.authored_id) {
        issues.push(monster_issue(
            &def.authored_id,
            "id",
            format!("invalid authored id ({err:?})"),
        ));
    }
    if let Err(err) = validate_authored_id(&def.sprite_id) {
        issues.push(monster_issue(
            &def.authored_id,
            "sprite",
            format!("invalid sprite id ({err:?})"),
        ));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
    }
}

/// Server-only gameplay definition. Placement and presentation are separate concerns.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub debug_name: String,
    pub health_max: f32,
    pub collision_bounds: MonsterCollisionBounds,
    pub movement_speed: f32,
    pub behavior: MonsterBehavior,
    pub home_leash_radius: f32,
}

pub fn validate_monster_definition(def: &MonsterDefinition) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    if let Err(err) = validate_authored_id(&def.authored_id) {
        issues.push(monster_issue(
            &def.authored_id,
            "id",
            format!("invalid authored id ({err:?})"),
        ));
    }
    if def.debug_name.trim().is_empty() {
        issues.push(monster_issue(
            &def.authored_id,
            "debug_name",
            "must not be empty",
        ));
    }
    positive_finite(&mut issues, def, "health_max", def.health_max);
    non_negative_finite(
        &mut issues,
        def,
        "collision_bounds.left",
        def.collision_bounds.left,
    );
    non_negative_finite(
        &mut issues,
        def,
        "collision_bounds.right",
        def.collision_bounds.right,
    );
    non_negative_finite(
        &mut issues,
        def,
        "collision_bounds.bottom",
        def.collision_bounds.bottom,
    );
    non_negative_finite(
        &mut issues,
        def,
        "collision_bounds.top",
        def.collision_bounds.top,
    );
    if def.collision_bounds.left + def.collision_bounds.right <= 0.0 {
        issues.push(monster_issue(
            &def.authored_id,
            "collision_bounds",
            "horizontal span must be greater than zero",
        ));
    }
    if def.collision_bounds.bottom + def.collision_bounds.top <= 0.0 {
        issues.push(monster_issue(
            &def.authored_id,
            "collision_bounds",
            "vertical span must be greater than zero",
        ));
    }
    positive_finite(&mut issues, def, "movement_speed", def.movement_speed);
    positive_finite(
        &mut issues,
        def,
        "behavior.home_leash_radius",
        def.home_leash_radius,
    );
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
    }
}

fn positive_finite(
    issues: &mut Vec<ValidationIssue>,
    def: &MonsterDefinition,
    field: &str,
    value: f32,
) {
    if !value.is_finite() || value <= 0.0 {
        issues.push(monster_issue(
            &def.authored_id,
            field,
            "must be finite and greater than zero",
        ));
    }
}

fn non_negative_finite(
    issues: &mut Vec<ValidationIssue>,
    def: &MonsterDefinition,
    field: &str,
    value: f32,
) {
    if !value.is_finite() || value < 0.0 {
        issues.push(monster_issue(
            &def.authored_id,
            field,
            "must be finite and non-negative",
        ));
    }
}

fn monster_issue(definition: &str, field: &str, detail: impl std::fmt::Display) -> ValidationIssue {
    ValidationIssue::new("monster", definition, field, detail.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_common::MONSTER_MOSS_CRAB;

    fn valid_presentation() -> MonsterPresentationDefinition {
        MonsterPresentationDefinition {
            content_id: MONSTER_MOSS_CRAB,
            authored_id: "monster.moss_crab".into(),
            sprite_id: "creature.moss_crab".into(),
        }
    }

    fn valid() -> MonsterDefinition {
        MonsterDefinition {
            content_id: MONSTER_MOSS_CRAB,
            authored_id: "monster.moss_crab".into(),
            debug_name: "Moss Crab".into(),
            health_max: 20.0,
            collision_bounds: MonsterCollisionBounds {
                left: 0.4,
                right: 0.4,
                bottom: 0.6,
                top: 0.6,
            },
            movement_speed: 2.0,
            behavior: MonsterBehavior::ChaseContactWhenAttacked,
            home_leash_radius: 3.0,
        }
    }

    #[test]
    fn valid_sprite_presentation_is_accepted() {
        validate_monster_presentation(&valid_presentation()).unwrap();
    }

    #[test]
    fn valid_chase_contact_definition_is_accepted() {
        validate_monster_definition(&valid()).unwrap();
    }

    #[test]
    fn invalid_runtime_values_are_rejected_together() {
        let mut def = valid();
        def.health_max = 0.0;
        def.movement_speed = f32::NAN;
        def.collision_bounds.bottom = 0.0;
        def.collision_bounds.top = 0.0;
        def.home_leash_radius = 0.0;
        let error = validate_monster_definition(&def).unwrap_err().to_string();
        assert!(error.contains("health_max"));
        assert!(error.contains("movement_speed"));
        assert!(error.contains("collision_bounds"));
        assert!(error.contains("home_leash_radius"));
    }
}
