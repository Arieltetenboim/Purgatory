//! Replication relationship / density / budget policy (Phase 6G.7C).
//!
//! Sits on the 6G.7B dirty fan-out path. Does not change wire protocol semantics.

use purgatory_simulation::{EntityId, ReplicationDirtyMask, World, point_in_aabb};

/// Env: `baseline` (6G.7B-equivalent) or `selective` (proof policy).
pub const REPLICATION_POLICY_ENV: &str = "PURGATORY_REPLICATION_POLICY";

/// Which dirty domains an observer is entitled to receive.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DomainEligibility {
    pub transform: bool,
    pub health: bool,
    pub equipment: bool,
}

impl DomainEligibility {
    #[must_use]
    pub fn any(self) -> bool {
        self.transform || self.health || self.equipment
    }
}

/// Content/map population hint (not sole authority).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum PopulationClass {
    #[default]
    Low,
    Medium,
    High,
    Extreme,
}

/// Combined runtime + hint pressure for adaptive cadence/budget.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum PressureLevel {
    #[default]
    Calm,
    Elevated,
    High,
    Extreme,
}

/// Extensible observer↔subject relationship (gameplay systems fill Party/Target later).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserverRelationKind {
    SelfObserver,
    Party,
    Target,
    NearbyStranger,
    DistantStranger,
}

/// Packer priority (lower = more important). Lifecycle handled outside Update path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ReplicationPriority {
    SelfState = 0,
    ImportantRelation = 1,
    NearbyMotion = 2,
    DistantVisible = 3,
    LowValue = 4,
}

/// State-like domains may coalesce / cadence; event-like must not be silently dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // documented distinction; Enter/Leave packed separately today
pub enum ReplicationSemantics {
    StateLike,
    EventLike,
}

#[must_use]
#[allow(dead_code)]
pub fn domain_semantics(transform: bool, health: bool) -> ReplicationSemantics {
    // Today's wire Update domains are both state-like. Lifecycle Enter/Leave are EventLike
    // and are packed before Updates in publish_observer_frame.
    let _ = (transform, health);
    ReplicationSemantics::StateLike
}

#[derive(Clone, Copy, Debug)]
pub struct PolicyDecision {
    pub eligibility: DomainEligibility,
    pub priority: ReplicationPriority,
    /// Cadence interval in ticks (1 = every tick when pending).
    pub cadence_interval: u64,
    /// Silent catch-up for ineligible lagged domains (visibility ≠ entitlement).
    pub suppress_emit: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PolicyMode {
    /// All visible domains eligible (6G.7B behavior).
    Baseline,
    /// Proof: stranger health suppressed; distant slower; priority packing.
    #[default]
    Selective,
}

impl PolicyMode {
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var(REPLICATION_POLICY_ENV)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "baseline" | "off" | "0" => Self::Baseline,
            _ => Self::Selective,
        }
    }
}

/// Optional proof overlays — not a party/target gameplay system.
#[derive(Debug, Default)]
pub struct RelationOverrides {
    pub party: HashMapPairs,
    pub target: HashMapTarget,
}

#[derive(Debug, Default)]
pub struct HashMapPairs {
    inner: std::collections::HashMap<EntityId, std::collections::HashSet<EntityId>>,
}

impl HashMapPairs {
    #[allow(dead_code)] // proof overlay for future party wiring / tests with live EntityIds
    pub fn insert(&mut self, observer: EntityId, subject: EntityId) {
        self.inner.entry(observer).or_default().insert(subject);
        self.inner.entry(subject).or_default().insert(observer);
    }

    #[must_use]
    pub fn contains(&self, observer: EntityId, subject: EntityId) -> bool {
        self.inner
            .get(&observer)
            .is_some_and(|s| s.contains(&subject))
    }
}

#[derive(Debug, Default)]
pub struct HashMapTarget {
    inner: std::collections::HashMap<EntityId, EntityId>,
}

impl HashMapTarget {
    #[allow(dead_code)]
    pub fn set(&mut self, observer: EntityId, target: EntityId) {
        self.inner.insert(observer, target);
    }

    #[must_use]
    pub fn get(&self, observer: EntityId) -> Option<EntityId> {
        self.inner.get(&observer).copied()
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // counts retained for future content-specific tables / metrics
pub struct PolicyContext {
    pub mode: PolicyMode,
    pub population: PopulationClass,
    pub pressure: PressureLevel,
    pub observer_known: u32,
    pub subject_interested: u32,
}

impl PolicyContext {
    #[must_use]
    #[cfg(test)]
    pub fn with_counts(mut self, observer_known: u32, subject_interested: u32) -> Self {
        self.observer_known = observer_known;
        self.subject_interested = subject_interested;
        let _ = self.population;
        self
    }

    #[must_use]
    pub fn classify_pressure(
        population: PopulationClass,
        observer_known: u32,
        subject_interested: u32,
        recent_bytes: u32,
        tick_overrun_hint: bool,
    ) -> PressureLevel {
        let mut score = 0u32;
        score += match population {
            PopulationClass::Low => 0,
            PopulationClass::Medium => 1,
            PopulationClass::High => 2,
            PopulationClass::Extreme => 3,
        };
        if observer_known >= 64 {
            score += 1;
        }
        if observer_known >= 128 {
            score += 1;
        }
        if subject_interested >= 32 {
            score += 1;
        }
        if subject_interested >= 64 {
            score += 1;
        }
        if recent_bytes >= 2048 {
            score += 1;
        }
        if recent_bytes >= 3500 {
            score += 1;
        }
        if tick_overrun_hint {
            score += 2;
        }
        match score {
            0..=1 => PressureLevel::Calm,
            2..=3 => PressureLevel::Elevated,
            4..=5 => PressureLevel::High,
            _ => PressureLevel::Extreme,
        }
    }
}

#[must_use]
pub fn classify_relation(
    world: &World,
    observer: EntityId,
    subject: EntityId,
    overrides: &RelationOverrides,
) -> ObserverRelationKind {
    if observer == subject {
        return ObserverRelationKind::SelfObserver;
    }
    if overrides.party.contains(observer, subject) {
        return ObserverRelationKind::Party;
    }
    if overrides.target.get(observer) == Some(subject) {
        return ObserverRelationKind::Target;
    }
    let Some(subject_tf) = world.transform_of(subject) else {
        return ObserverRelationKind::DistantStranger;
    };
    if world.transform_of(observer).is_none() {
        return ObserverRelationKind::DistantStranger;
    }
    if world.address_of(observer) != world.address_of(subject) {
        return ObserverRelationKind::DistantStranger;
    }
    // Nearby = inside the validated visible envelope + prefetch. A 10 wu
    // player-centered radius is smaller than the FOOTNOTE viewport, so using
    // it here made on-screen edge remotes Distant (cadence 4) while interior
    // remotes stayed Nearby (cadence 2).
    if let Some(rects) = world.aoi_rects_for(observer)
        && point_in_aabb(subject_tf.position, rects.enter)
    {
        return ObserverRelationKind::NearbyStranger;
    }
    ObserverRelationKind::DistantStranger
}

/// Decide domain eligibility / priority / cadence for one observer↔subject dirty edge.
#[must_use]
pub fn decide_update_policy(
    ctx: PolicyContext,
    relation: ObserverRelationKind,
    dirty: ReplicationDirtyMask,
) -> PolicyDecision {
    match ctx.mode {
        PolicyMode::Baseline => baseline_decision(relation, dirty),
        PolicyMode::Selective => selective_decision(ctx, relation, dirty),
    }
}

fn baseline_decision(
    relation: ObserverRelationKind,
    dirty: ReplicationDirtyMask,
) -> PolicyDecision {
    let priority = match relation {
        ObserverRelationKind::SelfObserver => ReplicationPriority::SelfState,
        ObserverRelationKind::Party | ObserverRelationKind::Target => {
            ReplicationPriority::ImportantRelation
        }
        ObserverRelationKind::NearbyStranger => ReplicationPriority::NearbyMotion,
        ObserverRelationKind::DistantStranger => ReplicationPriority::DistantVisible,
    };
    PolicyDecision {
        eligibility: DomainEligibility {
            transform: dirty.transform,
            health: dirty.health,
            equipment: dirty.equipment.any(),
        },
        priority,
        cadence_interval: 1,
        suppress_emit: false,
    }
}

fn selective_decision(
    ctx: PolicyContext,
    relation: ObserverRelationKind,
    dirty: ReplicationDirtyMask,
) -> PolicyDecision {
    let (eligibility, priority, mut interval) = match relation {
        ObserverRelationKind::SelfObserver => (
            DomainEligibility {
                transform: dirty.transform,
                health: dirty.health,
                equipment: dirty.equipment.any(),
            },
            ReplicationPriority::SelfState,
            1u64,
        ),
        ObserverRelationKind::Party | ObserverRelationKind::Target => (
            DomainEligibility {
                transform: dirty.transform,
                health: dirty.health,
                equipment: dirty.equipment.any(),
            },
            ReplicationPriority::ImportantRelation,
            1u64,
        ),
        ObserverRelationKind::NearbyStranger => (
            // Proof: strangers get motion, not health. Equipment is appearance.
            DomainEligibility {
                transform: dirty.transform,
                health: false,
                equipment: dirty.equipment.any(),
            },
            ReplicationPriority::NearbyMotion,
            2u64,
        ),
        ObserverRelationKind::DistantStranger => (
            DomainEligibility {
                transform: dirty.transform,
                health: false,
                equipment: dirty.equipment.any(),
            },
            ReplicationPriority::DistantVisible,
            4u64,
        ),
    };

    // Density/pressure stretches stranger cadence further (state-like only).
    if matches!(
        relation,
        ObserverRelationKind::NearbyStranger | ObserverRelationKind::DistantStranger
    ) {
        interval = match ctx.pressure {
            PressureLevel::Calm => interval,
            PressureLevel::Elevated => interval.saturating_mul(2).max(2),
            PressureLevel::High => interval.saturating_mul(3).max(3),
            PressureLevel::Extreme => interval.saturating_mul(4).max(4),
        };
        let cadence_mult = stranger_cadence_mult_from_env();
        if cadence_mult > 1 {
            interval = interval.saturating_mul(cadence_mult).max(cadence_mult);
        }
        if ctx.pressure >= PressureLevel::High
            && relation == ObserverRelationKind::DistantStranger
            && !dirty.transform
            && !dirty.equipment.any()
        {
            // No remaining eligible domain under extreme — suppress emit.
            return PolicyDecision {
                eligibility: DomainEligibility::default(),
                priority: ReplicationPriority::LowValue,
                cadence_interval: interval,
                suppress_emit: true,
            };
        }
    }

    let suppress_emit = !eligibility.any() && dirty.any();
    PolicyDecision {
        eligibility,
        priority,
        cadence_interval: interval.max(1),
        suppress_emit,
    }
}

/// Soft per-observer frame budget under pressure (still capped by hard max elsewhere).
#[must_use]
pub fn observer_frame_budget_bytes(base: usize, pressure: PressureLevel) -> usize {
    match pressure {
        PressureLevel::Calm => base,
        PressureLevel::Elevated => (base * 7) / 8,
        PressureLevel::High => (base * 5) / 8,
        PressureLevel::Extreme => base / 2,
    }
    .max(512)
}

/// Env: optional stranger cadence multiplier for selective policy (1–8). Default 1.
pub const REPLICATION_STRANGER_CADENCE_MULT_ENV: &str =
    "PURGATORY_REPLICATION_STRANGER_CADENCE_MULT";

#[must_use]
pub fn stranger_cadence_mult_from_env() -> u64 {
    std::env::var(REPLICATION_STRANGER_CADENCE_MULT_ENV)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(1)
        .clamp(1, 8)
}

#[must_use]
pub fn population_class_from_env() -> PopulationClass {
    match std::env::var("PURGATORY_POPULATION_CLASS")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "medium" => PopulationClass::Medium,
        "high" => PopulationClass::High,
        "extreme" => PopulationClass::Extreme,
        _ => PopulationClass::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selective_stranger_suppresses_health() {
        let ctx = PolicyContext {
            mode: PolicyMode::Selective,
            population: PopulationClass::High,
            pressure: PressureLevel::Elevated,
            observer_known: 80,
            subject_interested: 40,
        };
        let d = decide_update_policy(
            ctx,
            ObserverRelationKind::NearbyStranger,
            ReplicationDirtyMask {
                transform: true,
                health: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert!(d.eligibility.transform);
        assert!(!d.eligibility.health);
        assert!(d.cadence_interval >= 2);
    }

    #[test]
    fn baseline_keeps_stranger_health() {
        let ctx = PolicyContext {
            mode: PolicyMode::Baseline,
            population: PopulationClass::Low,
            pressure: PressureLevel::Calm,
            observer_known: 10,
            subject_interested: 5,
        };
        let d = decide_update_policy(
            ctx,
            ObserverRelationKind::NearbyStranger,
            ReplicationDirtyMask {
                transform: true,
                health: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert!(d.eligibility.health);
        assert_eq!(d.cadence_interval, 1);
    }

    #[test]
    fn self_always_full_domains() {
        let ctx = PolicyContext {
            mode: PolicyMode::Selective,
            population: PopulationClass::Extreme,
            pressure: PressureLevel::Extreme,
            observer_known: 200,
            subject_interested: 200,
        };
        let d = decide_update_policy(
            ctx,
            ObserverRelationKind::SelfObserver,
            ReplicationDirtyMask {
                transform: true,
                health: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert!(d.eligibility.transform && d.eligibility.health);
        assert_eq!(d.priority, ReplicationPriority::SelfState);
        assert_eq!(d.cadence_interval, 1);
    }

    #[test]
    fn party_relation_keeps_health_under_selective() {
        let ctx = PolicyContext {
            mode: PolicyMode::Selective,
            population: PopulationClass::Extreme,
            pressure: PressureLevel::Extreme,
            observer_known: 200,
            subject_interested: 200,
        };
        let d = decide_update_policy(
            ctx,
            ObserverRelationKind::Party,
            ReplicationDirtyMask {
                transform: true,
                health: true,
                ..ReplicationDirtyMask::default()
            },
        );
        assert!(d.eligibility.health);
        assert_eq!(d.priority, ReplicationPriority::ImportantRelation);
        assert_eq!(
            domain_semantics(true, true),
            ReplicationSemantics::StateLike
        );
        assert!(observer_frame_budget_bytes(4096, PressureLevel::Extreme) < 4096);
        assert_eq!(observer_frame_budget_bytes(4096, PressureLevel::Calm), 4096);
        let _ = ctx.with_counts(10, 10);
    }

    #[test]
    fn stranger_equipment_remains_eligible() {
        let ctx = PolicyContext {
            mode: PolicyMode::Selective,
            population: PopulationClass::High,
            pressure: PressureLevel::Elevated,
            observer_known: 80,
            subject_interested: 40,
        };
        let d = decide_update_policy(
            ctx,
            ObserverRelationKind::NearbyStranger,
            ReplicationDirtyMask::equipment_only(purgatory_simulation::EquipmentDirtyMask::only(
                purgatory_simulation::EquipmentSlot::Weapon,
            )),
        );
        assert!(d.eligibility.equipment);
        assert!(!d.eligibility.health);
        assert!(!d.suppress_emit);
    }
}
