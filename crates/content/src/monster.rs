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

/// 10_000 basis points is 100%. The UI edits percent; storage is this integer.
pub const DROP_CHANCE_BPS_MAX: u32 = 10_000;

/// One independent drop row. Chance is not a weight against the other rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonsterDropEntry {
    pub item: ContentId,
    pub chance_bps: u32,
    pub quantity_min: u32,
    pub quantity_max: u32,
}

/// One successful row after a single death roll. Quantity is already chosen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolledMonsterDrop {
    pub item: ContentId,
    pub quantity: u32,
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
    /// Empty means this monster authors no drops.
    pub drops: Vec<MonsterDropEntry>,
}

/// Roll each entry once. `next_unit` supplies raw entropy; this function reduces it.
/// A failed chance does not consume a second value. 0 never succeeds and 10_000 always does.
/// Uniform integer in `0..bound`. Rejects the incomplete top of the `u32` range
/// so a small bound does not collapse onto one residue of a generator's low bits.
pub fn uniform_below(next_unit: &mut impl FnMut() -> u32, bound: u32) -> u32 {
    assert!(bound > 0, "uniform bound must be positive");
    let limit = u32::MAX - (u32::MAX % bound);
    loop {
        let value = next_unit();
        if value < limit {
            return value % bound;
        }
    }
}

pub fn roll_monster_drops(
    entries: &[MonsterDropEntry],
    mut next_unit: impl FnMut() -> u32,
) -> Vec<RolledMonsterDrop> {
    let mut rolled = Vec::new();
    for entry in entries {
        let chance = uniform_below(&mut next_unit, DROP_CHANCE_BPS_MAX);
        if chance >= entry.chance_bps {
            continue;
        }
        let span = entry.quantity_max - entry.quantity_min;
        let quantity = if span == 0 {
            entry.quantity_min
        } else {
            entry.quantity_min + uniform_below(&mut next_unit, span + 1)
        };
        rolled.push(RolledMonsterDrop {
            item: entry.item,
            quantity,
        });
    }
    rolled
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
    validate_drop_shape(&mut issues, def);
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

fn validate_drop_shape(issues: &mut Vec<ValidationIssue>, def: &MonsterDefinition) {
    let mut seen = Vec::new();
    for (index, entry) in def.drops.iter().enumerate() {
        let field = format!("drops[{index}]");
        if entry.item.kind() != Some(purgatory_common::ContentKind::Item) {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                "item must be an allocated Item ContentId",
            ));
        }
        if seen.contains(&entry.item) {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                "duplicate item in this drop list",
            ));
        }
        seen.push(entry.item);
        if entry.chance_bps > DROP_CHANCE_BPS_MAX {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                "chance_bps must be from 0 through 10000",
            ));
        }
        if entry.quantity_min == 0 || entry.quantity_max < entry.quantity_min {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                "quantity must be a positive inclusive range",
            ));
        }
    }
}

/// Item identity and stack limits are known only after the item registry is loaded.
pub fn validate_monster_drop_items(
    def: &MonsterDefinition,
    stack_limit: impl Fn(ContentId) -> Option<u32>,
) -> Result<(), ContentError> {
    let mut issues = Vec::new();
    for (index, entry) in def.drops.iter().enumerate() {
        let field = format!("drops[{index}]");
        let Some(limit) = stack_limit(entry.item) else {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                format!("unknown item {}", entry.item),
            ));
            continue;
        };
        if entry.quantity_max > limit {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                format!("quantity_max exceeds item stack_limit {limit}"),
            ));
        }
        if limit == 1 && (entry.quantity_min != 1 || entry.quantity_max != 1) {
            issues.push(monster_issue(
                &def.authored_id,
                &field,
                "nonstackable items drop quantity 1",
            ));
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ContentError { issues })
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
            drops: Vec::new(),
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

    #[test]
    fn independent_rows_keep_their_own_chance_and_quantity() {
        let potion = purgatory_common::ITEM_SMALL_POTION;
        let scrap = purgatory_common::ITEM_IRON_SCRAP;
        let entries = [
            MonsterDropEntry {
                item: potion,
                chance_bps: 10_000,
                quantity_min: 1,
                quantity_max: 1,
            },
            MonsterDropEntry {
                item: scrap,
                chance_bps: 0,
                quantity_min: 1,
                quantity_max: 3,
            },
        ];
        let rolled = roll_monster_drops(&entries, || 0);
        assert_eq!(
            rolled,
            vec![RolledMonsterDrop {
                item: potion,
                quantity: 1
            }]
        );
    }

    #[test]
    fn variable_quantity_is_inclusive_and_one_hundred_percent_always_drops() {
        let scrap = purgatory_common::ITEM_IRON_SCRAP;
        let entries = [MonsterDropEntry {
            item: scrap,
            chance_bps: 10_000,
            quantity_min: 1,
            quantity_max: 3,
        }];
        let values = [0u32, 2];
        let mut index = 0;
        let rolled = roll_monster_drops(&entries, || {
            let value = values[index];
            index += 1;
            value
        });
        assert_eq!(rolled[0].quantity, 3);
    }
}
