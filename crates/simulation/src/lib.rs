//! Authoritative simulation core.
//!
//! This crate must remain free of windowing, GPU, networking, and client
//! presentation dependencies. Gameplay systems must not read wall-clock time;
//! callers supply elapsed [`std::time::Duration`] values to [`SimulationClock`].

mod aabb;
mod ability;
#[cfg(test)]
mod ability_runtime_behavior_tests;
mod action;
mod action_gate;
mod aoi;
#[cfg(test)]
mod basic_attack_behavior_tests;
mod body;
mod bounds;
mod cadence;
mod clock;
mod collision;
#[cfg(test)]
mod combat_presentation_behavior_tests;
mod command;
mod contact;
mod debug_action;
mod dirty;
mod domain;
mod effect;
mod entity;
mod equipment;
#[cfg(test)]
mod equipment_runtime_behavior_tests;
#[cfg(test)]
mod equipment_serialization_tests;
#[cfg(test)]
mod fixtures;
mod footnote;
mod health;
mod input;
mod input_gate;
mod interactable;
mod interaction;
mod interest_locality;
mod lifecycle;
mod map_runtime;
mod motion_debug;
mod movement;
mod npc;
#[cfg(test)]
mod npc_combat_behavior_tests;
#[cfg(test)]
mod phase10d3_tests;
#[cfg(test)]
mod phase47_tests;
#[cfg(test)]
mod phase60_tests;
#[cfg(test)]
mod phase6a_tests;
#[cfg(test)]
mod phase6b_tests;
#[cfg(test)]
mod phase6c_tests;
#[cfg(test)]
mod phase6d_tests;
#[cfg(test)]
mod phase6f_tests;
#[cfg(test)]
mod phase6g6_tests;
#[cfg(test)]
mod phase6g7a_tests;
#[cfg(test)]
mod phase6g_tests;
#[cfg(test)]
mod phase72_tests;
mod platform;
mod presentation_oneshot;
mod query;
mod replication;
mod runtime;
mod runtime_event;
mod runtime_stats;
mod scheduler;
mod spatial;
mod spawn;
mod spawn_schedule;
mod stage;
mod time;
mod transform;
mod world;

pub use aabb::Aabb;
pub use ability::{
    ABILITY_EFFECT_CAP, AbilityActivation, AbilityDefinition, AbilityDefinitionError,
    AbilityDelivery, AbilityEffect, AbilityGrantTable, AbilityId, AbilityRejectReason,
    AbilityRequest, AbilityTiming, CooldownTable, GameplayPresentationCue, cue_for_ability_cast,
    cue_for_damage_outcome, forward_query_aabb, oneshot_kind_for_cue,
};
pub use action::{Action, ActionEnd, ActionError, ActionId, ActionKind, ActionPhase};
pub use action_gate::{ActionDenialReason, ActionGateContext, evaluate_action_gate};
pub use aoi::{
    AOI_CAMERA_DEAD_ZONE_HALF, AOI_INFLUENCE_HALF_EXTENTS, AOI_LEAVE_MARGIN,
    AOI_POLICY_HALF_EXTENTS, AOI_PREFETCH_MARGIN, AOI_VIEWPORT_ASPECT, AoiRects,
    aoi_clamp_camera_center, aoi_policy_rects, aoi_view_envelope, aoi_viewport_size, point_in_aabb,
};
pub use body::{PLAYER_HALF_EXTENTS, PlayerBody, PlayerState};
pub use bounds::WorldBounds;
pub use cadence::{
    Cadence, CadenceBinding, CadenceKey, CadenceTable, cadence_due, staggered_interval_due,
};
pub use clock::{ClockConfig, ClockUpdate, SimulationClock};
pub use collision::{
    Overlap, RecoveryResult, VerticalContact, detect_overlap, detect_overlaps,
    recover_solid_penetration,
};
pub use command::{CommandClass, CommandDenial, validate_command_preamble};
pub use contact::{CONTACT_EPSILON, MAX_RECOVERY_TRANSLATION, RECOVERY_PENETRATION_MIN};
pub use debug_action::DebugAction;
pub use dirty::DirtyFlags;
pub use domain::{DomainRevs, ReplicationDirtyMask};
pub use effect::{EffectError, EffectId, EffectKind, TempEffect};
pub use entity::{EntityId, EntityKind, RuntimeEntityId};
pub use equipment::{
    EQUIPMENT_DELTA_EQUIP_BYTES, EQUIPMENT_DELTA_UNEQUIP_BYTES, EQUIPMENT_FULL_ALL_OCCUPIED_BYTES,
    EQUIPMENT_FULL_EMPTY_BYTES, EquipmentCodecError, EquipmentDelta, EquipmentDirtyMask,
    EquipmentSlot, EquipmentState, decode_equipment_delta, decode_equipment_full,
    encode_equipment_delta, encode_equipment_full,
};
pub use footnote::{BlockQuery, ContactEvent, FootnoteConfig, surface_blocks};
pub use health::{Health, PLAYER_HEALTH_MAX};
pub use input::PlayerInput;
pub use input_gate::{
    InputGateReason, MAP_TRANSITION_INPUT_LOCK_SECS, MEMBERSHIP_TRANSITION_INPUT_LOCK_SECS,
    map_transition_input_lock_ticks, membership_transition_input_lock_ticks,
};
pub use interactable::{
    INTERACT_RANGE, Interactable, InteractableKind, PORTAL_ACTIVATE_HALF, in_portal_activation_zone,
};
pub use interaction::{
    InteractionCloseReason, InteractionReject, InteractionSession, InteractionSessionId,
    InteractionSessionState,
};
pub use interest_locality::InterestLocalityAccounting;
pub use lifecycle::EntityLifecycle;
pub use map_runtime::{InstantiateError, InstantiatedMap, MapRuntimePlan, PlanPlatform};
pub use motion_debug::{CorrectionAxis, PlayerMotionDebug, ResponseKind};
pub use movement::{GRAVITY, JUMP_VELOCITY, MOVE_SPEED};
pub use npc::{
    ActionRejectReason, ActionRequest, NPC_HEALTH_MAX, NPC_MOVE_SPEED, NpcState, PULSE_DAMAGE,
    PULSE_DURATION_TICKS, PULSE_PERIOD_TICKS, STRIKE_DAMAGE, STRIKE_DURATION_TICKS, STRIKE_RANGE,
};
pub use platform::{
    Approach, FLOOR, FLOOR_POSITION, ONEWAY_A, ONEWAY_A_POSITION, ONEWAY_B, ONEWAY_B_POSITION,
    Platform, PlatformKind, PlatformView, RAISED_PLATFORM, RAISED_PLATFORM_POSITION,
};
pub use presentation_oneshot::{
    ATTACK_DURATION_TICKS, HURT_DURATION_TICKS, PresentationOneShot, PresentationOneShotError,
    PresentationOneShotKind, duration_ticks, oneshot_if_active, try_start_oneshot,
};
pub use purgatory_common::{ChannelId, ContentId, InstanceId, MapId, PersistentId, WorldAddress};
pub use query::{QueryFilter, QueryLimit};
pub use replication::{
    ReplicationClass, ReplicationMeta, ReplicationPayloadKind, UpdateFrequencyTier,
};
pub use runtime::DrainApplyTiming;
pub use runtime_event::{EventQueue, RuntimeEvent};
pub use runtime_stats::RuntimeStats;
pub use scheduler::{
    CRITICAL_DRAIN_CEILING, DEFERRED_DRAIN_BUDGET, DrainOutcome, FiredJob, SCHEDULER_CAPACITY,
    ScheduleOwner, ScheduledKind, Scheduler, TimerId, WorkLane,
};
pub use spatial::{SPATIAL_CELL_SIZE_WU, SpatialIndex, cell_of};
pub use spawn::RuntimeSpawnRequest;
pub use spawn_schedule::{ScheduledSpawn, SpawnRequestId};
pub use stage::{FOOTNOTE_SPAWN_X, FOOTNOTE_TEST_VIEWPORT_HEIGHT, P0, P0_POSITION};
pub use time::{
    MAX_CATCH_UP, MAX_CATCH_UP_NANOS, MAX_CATCH_UP_TICKS, MAX_TICKS_PER_ADVANCE, SimulationTick,
    SimulationTime, TICK_DURATION, TICK_DURATION_NANOS, TICK_RATE_HZ, ticks_from_duration,
};
pub use transform::Transform;
pub use world::World;

/// Cargo package version for this crate.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn common_is_linked() {
        assert!(!purgatory_common::version().is_empty());
    }

    #[test]
    fn movement_sources_do_not_import_winit_or_wgpu() {
        for src in [
            include_str!("aabb.rs"),
            include_str!("action.rs"),
            include_str!("action_gate.rs"),
            include_str!("aoi.rs"),
            include_str!("body.rs"),
            include_str!("bounds.rs"),
            include_str!("cadence.rs"),
            include_str!("collision.rs"),
            include_str!("command.rs"),
            include_str!("contact.rs"),
            include_str!("debug_action.rs"),
            include_str!("dirty.rs"),
            include_str!("domain.rs"),
            include_str!("effect.rs"),
            include_str!("entity.rs"),
            include_str!("equipment.rs"),
            include_str!("footnote/mod.rs"),
            include_str!("footnote/contact.rs"),
            include_str!("footnote/controller.rs"),
            include_str!("footnote/surface.rs"),
            include_str!("health.rs"),
            include_str!("input.rs"),
            include_str!("interactable.rs"),
            include_str!("interaction.rs"),
            include_str!("lifecycle.rs"),
            include_str!("map_runtime.rs"),
            include_str!("motion_debug.rs"),
            include_str!("movement.rs"),
            include_str!("npc.rs"),
            include_str!("platform.rs"),
            include_str!("query.rs"),
            include_str!("replication.rs"),
            include_str!("runtime.rs"),
            include_str!("runtime_event.rs"),
            include_str!("runtime_stats.rs"),
            include_str!("scheduler.rs"),
            include_str!("spawn.rs"),
            include_str!("spatial.rs"),
            include_str!("spawn_schedule.rs"),
            include_str!("stage.rs"),
            include_str!("transform.rs"),
            include_str!("world.rs"),
        ] {
            assert!(
                !src.contains("use winit") && !src.contains("extern crate winit"),
                "simulation must not import winit"
            );
            assert!(
                !src.contains("use wgpu") && !src.contains("extern crate wgpu"),
                "simulation must not import wgpu"
            );
            assert!(
                !src.contains("use egui") && !src.contains("extern crate egui"),
                "simulation must not import egui"
            );
        }
    }
}
