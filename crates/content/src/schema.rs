//! Typed content definitions. Gameplay never sees raw JSON.

use crate::domain::ContentDomain;
use purgatory_common::ContentId;
use purgatory_simulation::{InteractableKind, WorldBounds};

pub const CONTENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct EntityDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub debug_name: String,
    pub domain: ContentDomain,
    pub visible: bool,
    pub interactable: Option<InteractableKind>,
    pub transition: Option<TransitionRef>,
}

#[derive(Clone, Debug)]
pub struct TransitionRef {
    pub map_authored: String,
    pub portal_authored: String,
}

#[derive(Clone, Debug)]
pub struct SpawnPoint {
    pub id: String,
    pub position: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct MapPlatform {
    pub position: [f32; 2],
    pub half_extents: [f32; 2],
    pub kind: purgatory_simulation::PlatformKind,
}

#[derive(Clone, Debug)]
pub struct MapDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub debug_name: String,
    pub domain: ContentDomain,
    pub bounds: WorldBounds,
    pub spawn_points: Vec<SpawnPoint>,
    pub platforms: Vec<MapPlatform>,
    pub restore: RestorePolicy,
}

/// Authored restore policy. Not a WorldAddress and not Channel/Instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestorePolicy {
    SafePoint {
        point_id: String,
    },
    Checkpoint {
        point_id: String,
    },
    NonReenterable {
        fallback_map: String,
        fallback_point: String,
    },
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub entity_authored: String,
    pub position: [f32; 2],
}
