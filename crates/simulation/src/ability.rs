//! Ability contracts (Phase 9A). Lifecycle owner is [`crate::action::ActionTable`].
//!
//! Not a parallel ability state machine. Not a GAS. Instant Health mutation
//! goes through [`AbilityEffect`], never through ability-specific `set_health`.
//! [`crate::effect::TempEffect`] remains duration-based (Pulse / later status).

use crate::action::ActionId;
use crate::entity::EntityId;
use crate::presentation_oneshot::PresentationOneShotKind;
use crate::time::SimulationTick;
use purgatory_common::{ContentId, ItemInstanceId};

/// Stable authored ability identity. Same type as other content ids.
pub type AbilityId = ContentId;

/// Maximum ordered effects on one v1 definition. Raise later if a real ability needs more.
pub const ABILITY_EFFECT_CAP: usize = 4;

/// Authored timing in simulation ticks (30 Hz). Zero skips that live phase.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AbilityTiming {
    pub windup_ticks: u64,
    pub active_ticks: u64,
    pub recovery_ticks: u64,
    pub cooldown_ticks: u64,
}

/// How the ability is **activated**. Distinct from who is later affected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityActivation {
    /// No selected entity required. Basic Attack.
    Independent,
    /// Future: a selected entity is required to start. Not Basic Attack.
    SelectedEntity,
}

/// How affected entities are chosen at Active. Not an activation requirement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AbilityDelivery {
    /// Forward AABB along owner facing. Zero hits is a valid Active.
    ForwardQuery {
        range: f32,
        half_height: f32,
        max_targets: u8,
    },
    /// Affect `AbilityRequest.selected` at Active if still a valid hit.
    SelectedEntity,
}

/// Instant ability effect kind. Closed enum: add variants later, do not flatten
/// Heal/Buff into [`AbilityDefinition`] fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AbilityEffect {
    /// Flat damage. Executed via [`crate::World::apply_damage`], not ability code.
    Damage { amount: f32 },
}

/// Minimum content-driven ability shape. Shared JSON lives in `content/shared/abilities/`.
#[derive(Clone, Debug, PartialEq)]
pub struct AbilityDefinition {
    pub id: AbilityId,
    pub timing: AbilityTiming,
    pub activation: AbilityActivation,
    pub delivery: AbilityDelivery,
    pub effects: Vec<AbilityEffect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityDefinitionError {
    EmptyEffects,
    TooManyEffects,
    InvalidDamage,
    InvalidRange,
    InvalidHalfHeight,
    InvalidMaxTargets,
}

impl AbilityDefinition {
    pub fn validate(&self) -> Result<(), AbilityDefinitionError> {
        if self.effects.is_empty() {
            return Err(AbilityDefinitionError::EmptyEffects);
        }
        if self.effects.len() > ABILITY_EFFECT_CAP {
            return Err(AbilityDefinitionError::TooManyEffects);
        }
        for effect in &self.effects {
            match *effect {
                AbilityEffect::Damage { amount } => {
                    if !amount.is_finite() || amount <= 0.0 {
                        return Err(AbilityDefinitionError::InvalidDamage);
                    }
                }
            }
        }
        match self.delivery {
            AbilityDelivery::ForwardQuery {
                range,
                half_height,
                max_targets,
            } => {
                if !range.is_finite() || range <= 0.0 {
                    return Err(AbilityDefinitionError::InvalidRange);
                }
                if !half_height.is_finite() || half_height <= 0.0 {
                    return Err(AbilityDefinitionError::InvalidHalfHeight);
                }
                if max_targets == 0 {
                    return Err(AbilityDefinitionError::InvalidMaxTargets);
                }
            }
            AbilityDelivery::SelectedEntity => {}
        }
        Ok(())
    }

    #[must_use]
    pub fn initial_phase(&self) -> crate::action::ActionPhase {
        use crate::action::ActionPhase;
        if self.timing.windup_ticks > 0 {
            ActionPhase::Windup
        } else if self.timing.active_ticks > 0 || !self.effects.is_empty() {
            ActionPhase::Active
        } else if self.timing.recovery_ticks > 0 {
            ActionPhase::Recovery
        } else {
            ActionPhase::Active
        }
    }
}

/// Forward hit volume. Facing is `-1` (left) or `+1` (right). Origin is the owner position.
#[must_use]
pub fn forward_query_aabb(
    origin: [f32; 2],
    facing_x: f32,
    range: f32,
    half_height: f32,
) -> crate::aabb::Aabb {
    let sign = if facing_x < 0.0 { -1.0 } else { 1.0 };
    let (min_x, max_x) = if sign < 0.0 {
        (origin[0] - range, origin[0])
    } else {
        (origin[0], origin[0] + range)
    };
    crate::aabb::Aabb::from_min_max(
        min_x,
        origin[1] - half_height,
        max_x,
        origin[1] + half_height,
    )
}

/// Simulation-level ability request. Not a wire `ClientControl`.
///
/// `selected` is optional activation data. It is not a hit result and is not
/// required for [`AbilityActivation::Independent`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityRequest<'a> {
    pub actor: EntityId,
    pub selected: Option<EntityId>,
    pub definition: &'a AbilityDefinition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityRejectReason {
    InvalidDefinition,
    MissingActor,
    MissingTarget,
    ActorDead,
    TargetDead,
    OutOfRange,
    OnCooldown,
    Busy,
    Gate(crate::action_gate::ActionDenialReason),
}

impl AbilityRejectReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidDefinition => "InvalidDefinition",
            Self::MissingActor => "MissingActor",
            Self::MissingTarget => "MissingTarget",
            Self::ActorDead => "ActorDead",
            Self::TargetDead => "TargetDead",
            Self::OutOfRange => "OutOfRange",
            Self::OnCooldown => "OnCooldown",
            Self::Busy => "Busy",
            Self::Gate(_) => "Gate",
        }
    }
}

/// Semantic presentation cue. Character Presentation maps Attack/Hurt to clips.
/// Combat/ability code must not name animation clips or bones.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameplayPresentationCue {
    Attack,
    Hurt,
    /// Derived from Health.current <= 0. Persistent Character Presentation
    /// activity — not a Phase 8 oneshot.
    Dead,
}

#[must_use]
pub const fn cue_for_ability_cast() -> GameplayPresentationCue {
    GameplayPresentationCue::Attack
}

#[must_use]
pub const fn cue_for_damage_outcome(target_dead: bool) -> GameplayPresentationCue {
    if target_dead {
        GameplayPresentationCue::Dead
    } else {
        GameplayPresentationCue::Hurt
    }
}

/// Phase 8 oneshot vocabulary. `Dead` is Health-replicated, not a oneshot.
#[must_use]
pub const fn oneshot_kind_for_cue(cue: GameplayPresentationCue) -> Option<PresentationOneShotKind> {
    match cue {
        GameplayPresentationCue::Attack => Some(PresentationOneShotKind::Attack),
        GameplayPresentationCue::Hurt => Some(PresentationOneShotKind::Hurt),
        GameplayPresentationCue::Dead => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AbilityLive {
    pub action_id: ActionId,
    pub owner: EntityId,
    pub selected: Option<EntityId>,
    pub delivery: AbilityDelivery,
    pub effects: [AbilityEffect; ABILITY_EFFECT_CAP],
    pub effect_count: u8,
    pub active_ticks: u64,
    pub recovery_ticks: u64,
    pub effects_applied: bool,
}

impl AbilityLive {
    pub(crate) fn from_definition(
        action_id: ActionId,
        owner: EntityId,
        selected: Option<EntityId>,
        def: &AbilityDefinition,
    ) -> Self {
        let mut effects = [AbilityEffect::Damage { amount: 1.0 }; ABILITY_EFFECT_CAP];
        let n = def.effects.len().min(ABILITY_EFFECT_CAP);
        effects[..n].copy_from_slice(&def.effects[..n]);
        Self {
            action_id,
            owner,
            selected,
            delivery: def.delivery,
            effects,
            effect_count: n as u8,
            active_ticks: def.timing.active_ticks,
            recovery_ticks: def.timing.recovery_ticks,
            effects_applied: false,
        }
    }

    pub(crate) fn effects(&self) -> &[AbilityEffect] {
        &self.effects[..self.effect_count as usize]
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AbilityRuntimeTable {
    live: Vec<AbilityLive>,
}

impl AbilityRuntimeTable {
    #[must_use]
    pub fn new() -> Self {
        Self { live: Vec::new() }
    }

    pub fn insert(&mut self, live: AbilityLive) {
        self.live.push(live);
    }

    pub fn get_mut(&mut self, action_id: ActionId) -> Option<&mut AbilityLive> {
        self.live.iter_mut().find(|l| l.action_id == action_id)
    }

    pub fn remove(&mut self, action_id: ActionId) -> Option<AbilityLive> {
        let idx = self.live.iter().position(|l| l.action_id == action_id)?;
        Some(self.live.swap_remove(idx))
    }

    pub fn drop_owner(&mut self, owner: EntityId) {
        self.live.retain(|l| l.owner != owner);
    }
}

/// Server-only per-(owner, ability) ready tick. Not an entity component.
#[derive(Clone, Debug, Default)]
pub struct CooldownTable {
    ready_at: Vec<(EntityId, AbilityId, SimulationTick)>,
}

impl CooldownTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ready_at: Vec::new(),
        }
    }

    #[must_use]
    pub fn ready_at(&self, owner: EntityId, id: AbilityId) -> Option<SimulationTick> {
        self.ready_at
            .iter()
            .find(|(e, a, _)| *e == owner && *a == id)
            .map(|(_, _, t)| *t)
    }

    #[must_use]
    pub fn is_ready(&self, owner: EntityId, id: AbilityId, now: SimulationTick) -> bool {
        match self.ready_at(owner, id) {
            None => true,
            Some(ready) => now.get() >= ready.get(),
        }
    }

    pub fn set(&mut self, owner: EntityId, id: AbilityId, ready: SimulationTick) {
        if let Some(slot) = self
            .ready_at
            .iter_mut()
            .find(|(e, a, _)| *e == owner && *a == id)
        {
            slot.2 = ready;
            return;
        }
        self.ready_at.push((owner, id, ready));
    }

    pub fn drop_owner(&mut self, owner: EntityId) {
        self.ready_at.retain(|(e, _, _)| *e != owner);
    }
}

/// Explicit ability grants. Not a skill book. Future sources (progression,
/// equipment, status) write through this table.
#[derive(Clone, Debug, Default)]
pub struct AbilityGrantTable {
    grants: Vec<(EntityId, AbilityId, AbilityGrantSource)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbilityGrantSource {
    Intrinsic,
    Equipment(ItemInstanceId),
}

impl AbilityGrantTable {
    #[must_use]
    pub fn new() -> Self {
        Self { grants: Vec::new() }
    }

    #[must_use]
    pub fn contains(&self, owner: EntityId, id: AbilityId) -> bool {
        self.grants.iter().any(|(e, a, _)| *e == owner && *a == id)
    }

    pub fn insert(&mut self, owner: EntityId, id: AbilityId) {
        self.insert_from_source(owner, id, AbilityGrantSource::Intrinsic);
    }

    pub fn insert_from_source(
        &mut self,
        owner: EntityId,
        id: AbilityId,
        source: AbilityGrantSource,
    ) {
        if !self
            .grants
            .iter()
            .any(|(e, a, existing)| *e == owner && *a == id && *existing == source)
        {
            self.grants.push((owner, id, source));
        }
    }

    pub fn remove(&mut self, owner: EntityId, id: AbilityId) {
        self.grants.retain(|(e, a, source)| {
            !(*e == owner && *a == id && *source == AbilityGrantSource::Intrinsic)
        });
    }

    pub fn remove_from_source(
        &mut self,
        owner: EntityId,
        id: AbilityId,
        source: AbilityGrantSource,
    ) {
        self.grants
            .retain(|(e, a, existing)| !(*e == owner && *a == id && *existing == source));
    }

    pub fn drop_owner(&mut self, owner: EntityId) {
        self.grants.retain(|(e, _, _)| *e != owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionPhase;

    fn sample_def() -> AbilityDefinition {
        AbilityDefinition {
            id: ContentId::from_authored("skill.basic.strike").unwrap(),
            timing: AbilityTiming {
                windup_ticks: 2,
                active_ticks: 1,
                recovery_ticks: 3,
                cooldown_ticks: 10,
            },
            activation: AbilityActivation::Independent,
            delivery: AbilityDelivery::ForwardQuery {
                range: 1.5,
                half_height: 0.8,
                max_targets: 8,
            },
            effects: vec![AbilityEffect::Damage { amount: 5.0 }],
        }
    }

    #[test]
    fn definition_validates_minimum_shape() {
        sample_def().validate().unwrap();
    }

    #[test]
    fn definition_rejects_empty_or_bad_damage() {
        let mut def = sample_def();
        def.effects.clear();
        assert_eq!(def.validate(), Err(AbilityDefinitionError::EmptyEffects));
        def.effects = vec![AbilityEffect::Damage { amount: 0.0 }];
        assert_eq!(def.validate(), Err(AbilityDefinitionError::InvalidDamage));
        def.effects = vec![AbilityEffect::Damage { amount: 1.0 }; 5];
        assert_eq!(def.validate(), Err(AbilityDefinitionError::TooManyEffects));
    }

    #[test]
    fn windup_is_initial_live_phase() {
        assert_eq!(sample_def().initial_phase(), ActionPhase::Windup);
        let mut instant = sample_def();
        instant.timing.windup_ticks = 0;
        assert_eq!(instant.initial_phase(), ActionPhase::Active);
    }

    #[test]
    fn presentation_cues_do_not_name_clips() {
        assert_eq!(cue_for_ability_cast(), GameplayPresentationCue::Attack);
        assert_eq!(
            oneshot_kind_for_cue(cue_for_ability_cast()),
            Some(PresentationOneShotKind::Attack)
        );
        assert_eq!(
            oneshot_kind_for_cue(cue_for_damage_outcome(false)),
            Some(PresentationOneShotKind::Hurt)
        );
        assert_eq!(oneshot_kind_for_cue(cue_for_damage_outcome(true)), None);
    }

    #[test]
    fn cooldown_is_per_owner_ability_not_a_component() {
        let mut table = CooldownTable::new();
        let owner = EntityId::from_raw(1, 1);
        let id = ContentId::from_token(9);
        let now = SimulationTick::from_count(5);
        assert!(table.is_ready(owner, id, now));
        table.set(owner, id, SimulationTick::from_count(12));
        assert!(!table.is_ready(owner, id, now));
        assert!(table.is_ready(owner, id, SimulationTick::from_count(12)));
        table.drop_owner(owner);
        assert!(table.is_ready(owner, id, now));
    }

    #[test]
    fn forward_query_aabb_is_in_front_only() {
        let box_right = forward_query_aabb([0.0, 1.0], 1.0, 1.5, 0.8);
        assert!(box_right.contains_point([1.0, 1.0]));
        assert!(!box_right.contains_point([-0.1, 1.0]));
        let box_left = forward_query_aabb([0.0, 1.0], -1.0, 1.5, 0.8);
        assert!(box_left.contains_point([-1.0, 1.0]));
        assert!(!box_left.contains_point([0.1, 1.0]));
    }

    #[test]
    fn grant_table_is_per_owner_ability() {
        let mut table = AbilityGrantTable::new();
        let owner = EntityId::from_raw(1, 1);
        let id = ContentId::from_token(9);
        assert!(!table.contains(owner, id));
        table.insert(owner, id);
        table.insert(owner, id);
        assert!(table.contains(owner, id));
        table.remove(owner, id);
        assert!(!table.contains(owner, id));
        table.insert(owner, id);
        table.drop_owner(owner);
        assert!(!table.contains(owner, id));
    }

    #[test]
    fn removing_equipment_source_preserves_intrinsic_grant() {
        let mut table = AbilityGrantTable::new();
        let owner = EntityId::from_raw(1, 1);
        let ability = ContentId::from_token(9);
        let item = ItemInstanceId::from_raw(42);
        table.insert(owner, ability);
        table.insert_from_source(owner, ability, AbilityGrantSource::Equipment(item));
        table.remove_from_source(owner, ability, AbilityGrantSource::Equipment(item));
        assert!(table.contains(owner, ability));
        table.remove(owner, ability);
        assert!(!table.contains(owner, ability));
    }
}
