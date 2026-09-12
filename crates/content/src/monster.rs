//! Server-only authored monster definitions.

use crate::error::{ContentError, ValidationIssue};
use purgatory_common::{ContentId, validate_authored_id};

/// Monster content schema v1.
pub const MONSTER_CONTENT_SCHEMA_VERSION: u32 = 1;

/// The intentionally small behavior vocabulary supported by Monster schema v1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonsterBehavior {
    /// Use the existing authoritative player acquisition, approach, and contact loop.
    ChaseContact,
}

/// Server-only gameplay definition. Placement and presentation are separate concerns.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub debug_name: String,
    pub health_max: f32,
    pub half_extents: [f32; 2],
    pub movement_speed: f32,
    pub behavior: MonsterBehavior,
    pub acquisition_radius: f32,
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
        issues.push(monster_issue(&def.authored_id, "debug_name", "must not be empty"));
    }
    positive_finite(&mut issues, def, "health_max", def.health_max);
    positive_finite(&mut issues, def, "half_extents[0]", def.half_extents[0]);
    positive_finite(&mut issues, def, "half_extents[1]", def.half_extents[1]);
    positive_finite(&mut issues, def, "movement_speed", def.movement_speed);
    positive_finite(
        &mut issues,
        def,
        "behavior.acquisition_radius",
        def.acquisition_radius,
    );
    positive_finite(
        &mut issues,
        def,
        "behavior.home_leash_radius",
        def.home_leash_radius,
    );
    if def.home_leash_radius < def.acquisition_radius {
        issues.push(monster_issue(
            &def.authored_id,
            "behavior.home_leash_radius",
            "must be greater than or equal to acquisition_radius",
        ));
    }
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

fn monster_issue(
    definition: &str,
    field: &str,
    detail: impl std::fmt::Display,
) -> ValidationIssue {
    ValidationIssue::new("monster", definition, field, detail.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_common::MONSTER_RED_SLIME;

    fn valid() -> MonsterDefinition {
        MonsterDefinition {
            content_id: MONSTER_RED_SLIME,
            authored_id: "monster.slime.red".into(),
            debug_name: "Red Slime".into(),
            health_max: 20.0,
            half_extents: [0.4, 0.6],
            movement_speed: 2.0,
            behavior: MonsterBehavior::ChaseContact,
            acquisition_radius: 3.0,
            home_leash_radius: 3.0,
        }
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
        def.home_leash_radius = 1.0;
        let error = validate_monster_definition(&def).unwrap_err().to_string();
        assert!(error.contains("health_max"));
        assert!(error.contains("movement_speed"));
        assert!(error.contains("home_leash_radius"));
    }
}
