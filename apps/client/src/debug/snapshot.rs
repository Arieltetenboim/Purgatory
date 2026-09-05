//! Domain physics / roster / probe types for diagnostics composition.
//! Not authoritative world state. Not an overlay view-model.

use purgatory_simulation::{
    ContactEvent, ContentId, DirtyFlags, EntityId, EntityKind, EntityLifecycle, InteractableKind,
    PlatformKind, PlayerInput, PlayerMotionDebug, ReplicationClass, World, WorldAddress,
    WorldBounds,
};

use super::entity_inspector::WorldEntityInput;

/// Read-only player fields copied for one diagnostics frame.
#[derive(Clone, Copy, Debug)]
pub struct PlayerDebug {
    pub id: EntityId,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub grounded: bool,
    pub grounded_on: Option<EntityId>,
    pub ignored_platform: Option<EntityId>,
    pub last_contact: ContactEvent,
    pub grounded_kind: Option<PlatformKind>,
    pub address: WorldAddress,
    pub lifecycle: EntityLifecycle,
    pub content_id: Option<ContentId>,
    pub has_persistent_id: bool,
    pub replication: ReplicationClass,
}

/// FOOTNOTE section of physics diagnostics.
#[derive(Clone, Copy, Debug)]
pub struct FootnoteDebug {
    pub grounded: bool,
    pub grounded_on: Option<EntityId>,
    pub platform_kind: Option<PlatformKind>,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub horizontal_speed: f32,
    pub move_axis: i8,
    pub down_held: bool,
    pub ignored_platform: Option<EntityId>,
    pub last_contact: ContactEvent,
}

/// Read-only runtime row for the debug overlay. Not authoritative gameplay.
#[derive(Clone, Copy, Debug)]
pub struct EntityRuntimeDebug {
    pub id: EntityId,
    pub kind: EntityKind,
    pub address: WorldAddress,
    pub lifecycle: EntityLifecycle,
    pub content_id: Option<ContentId>,
    pub has_persistent_id: bool,
    pub replication: ReplicationClass,
    pub has_transform: bool,
    pub has_health: bool,
    pub dirty: DirtyFlags,
    pub interactable: Option<InteractableKind>,
}

impl From<&EntityRuntimeDebug> for WorldEntityInput {
    fn from(row: &EntityRuntimeDebug) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            address: row.address,
            lifecycle: row.lifecycle,
            content_id: row.content_id,
            has_persistent_id: row.has_persistent_id,
            replication: row.replication,
            has_transform: row.has_transform,
            has_health: row.has_health,
            dirty: row.dirty,
            interactable: row.interactable,
        }
    }
}

/// Local physics / FOOTNOTE copy from client `World`.
#[derive(Debug, Default)]
pub struct PhysicsDiagnostics {
    pub player: Option<PlayerDebug>,
    pub footnote: Option<FootnoteDebug>,
    pub motion: PlayerMotionDebug,
}

/// Client World roster (slots, counts, bounds). Not observer Known-set.
#[derive(Debug)]
pub struct WorldRosterDiagnostics {
    pub entity_count: u32,
    pub player_count: u32,
    pub platform_count: u32,
    pub bounds: WorldBounds,
    pub entities: Vec<EntityRuntimeDebug>,
}

impl Default for WorldRosterDiagnostics {
    fn default() -> Self {
        Self {
            entity_count: 0,
            player_count: 0,
            platform_count: 0,
            bounds: WorldBounds::DEV_COMPACT,
            entities: Vec::new(),
        }
    }
}

impl WorldRosterDiagnostics {
    /// Single World iteration for physics + roster. Skip unless a diagnostics consumer is active.
    #[must_use]
    pub fn from_world(world: &World, input: PlayerInput) -> (PhysicsDiagnostics, Self) {
        let player = world.player_body().map(|body| {
            let grounded_kind = body
                .grounded_on
                .and_then(|id| world.get_platform(id).map(|(_, p)| p.kind));
            PlayerDebug {
                id: body.id,
                position: body.position,
                velocity: body.velocity,
                grounded: body.grounded,
                grounded_on: body.grounded_on,
                ignored_platform: body.ignored_platform,
                last_contact: body.last_contact,
                grounded_kind,
                address: world.address_of(body.id).unwrap_or(WorldAddress::DEV),
                lifecycle: world
                    .lifecycle_of(body.id)
                    .unwrap_or(EntityLifecycle::Active),
                content_id: world.content_id_of(body.id),
                has_persistent_id: world.persistent_id_of(body.id).is_some(),
                replication: world
                    .replication_of(body.id)
                    .map(|m| m.class)
                    .unwrap_or(ReplicationClass::None),
            }
        });
        let footnote = player.map(|p| FootnoteDebug {
            grounded: p.grounded,
            grounded_on: p.grounded_on,
            platform_kind: p.grounded_kind,
            position: p.position,
            velocity: p.velocity,
            horizontal_speed: p.velocity[0].abs(),
            move_axis: input.move_axis,
            down_held: input.down_held,
            ignored_platform: p.ignored_platform,
            last_contact: p.last_contact,
        });
        let physics = PhysicsDiagnostics {
            player,
            footnote,
            motion: world.last_motion_debug(),
        };
        let roster = Self {
            entity_count: world.len(),
            player_count: world.iter_kind(EntityKind::Player).count() as u32,
            platform_count: world.iter_kind(EntityKind::Platform).count() as u32,
            bounds: world.bounds(),
            entities: world
                .iter()
                .filter_map(|id| {
                    Some(EntityRuntimeDebug {
                        id,
                        kind: world.kind(id)?,
                        address: world.address_of(id)?,
                        lifecycle: world.lifecycle_of(id)?,
                        content_id: world.content_id_of(id),
                        has_persistent_id: world.persistent_id_of(id).is_some(),
                        replication: world.replication_of(id)?.class,
                        has_transform: world.transform_of(id).is_some(),
                        has_health: world.health_of(id).is_some(),
                        dirty: world.dirty_of(id).unwrap_or_default(),
                        interactable: world.interactable_of(id).map(|i| i.kind),
                    })
                })
                .collect(),
        };
        (physics, roster)
    }
}

/// Overlay readout: selected bone Local → World → Screen. Not a renderer contract.
#[derive(Clone, Debug)]
pub struct SkeletonInspectDebug {
    pub index: u8,
    pub name: String,
    pub local_t: [f32; 2],
    pub local_r: f32,
    pub world_t: [f32; 2],
    pub world_r: f32,
    pub screen: Option<[f32; 2]>,
}

/// One remote player: replica vs interpolated vs presentation draw pose.
/// Temporary 8E motion-validation probe. Not a gameplay contract.
#[derive(Clone, Copy, Debug, Default)]
pub struct RemoteMotionProbe {
    pub entity_index: Option<u32>,
    pub entity_generation: Option<u32>,
    pub attachments: u32,
    pub used_interp: bool,
    pub auth: Option<[f32; 2]>,
    pub interp: Option<[f32; 2]>,
    pub presented: Option<[f32; 2]>,
    pub presented_root: Option<[f32; 2]>,
    pub dx_auth_interp: Option<f32>,
    pub dx_interp_presented: Option<f32>,
    pub aoi_band: Option<&'static str>,
    pub observer_distance: Option<f32>,
    pub last_auth_transform_tick: Option<u64>,
    pub effective_received_gap: Option<u64>,
    pub max_received_gap: Option<u64>,
    pub unique_history_ticks: [u64; 8],
    pub unique_history_len: u8,
    pub unique_count: u32,
    pub unique_oldest_tick: Option<u64>,
    pub unique_newest_tick: Option<u64>,
    pub entity_bracket_a: Option<u64>,
    pub entity_bracket_b: Option<u64>,
    pub entity_alpha: f32,
    pub entity_clamped_newest: bool,
    pub history_with_entity: u32,
    pub entity_oldest_tick: Option<u64>,
    pub entity_newest_tick: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_simulation::{EntityLifecycle, World, WorldAddress};

    #[test]
    fn world_roster_from_world_is_one_walk() {
        let world = World::dev_stage();
        let body = world.player_body().expect("player");
        let (physics, roster) = WorldRosterDiagnostics::from_world(&world, PlayerInput::idle());
        assert_eq!(roster.entity_count, world.len());
        assert!(!roster.entities.is_empty());
        assert_eq!(physics.player.expect("player").id, body.id);
        assert_eq!(physics.player.expect("player").address, WorldAddress::DEV);
        assert_eq!(
            physics.player.expect("player").lifecycle,
            EntityLifecycle::Active
        );
        let footnote = physics.footnote.expect("footnote");
        assert!(footnote.grounded);
        assert_eq!(footnote.move_axis, 0);
    }

    #[test]
    fn snapshot_module_has_no_windowing_tokens() {
        let src = include_str!("snapshot.rs");
        assert!(!src.contains(concat!("use w", "gpu")));
        assert!(!src.contains(concat!("use w", "init")));
        assert!(!src.contains(concat!("use e", "gui")));
        assert!(
            !src.contains(concat!("struct Debug", "Snapshot")),
            "D3 deleted the flat overlay adapter; do not reintroduce it"
        );
    }
}
