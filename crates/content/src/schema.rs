//! Typed content definitions. Gameplay never sees raw JSON.

use crate::domain::ContentDomain;
use crate::map_gameplay_authoring::FootholdPath;
use purgatory_common::ContentId;
use purgatory_simulation::{InteractableKind, WorldBounds};

pub const CONTENT_SCHEMA_VERSION: u32 = 1;
pub const PLACEMENT_SCHEMA_VERSION: u32 = 2;

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
    pub foothold_paths: Vec<FootholdPath>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementKind {
    Entity,
    Monster,
    Portal,
}

impl PlacementKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Monster => "monster",
            Self::Portal => "portal",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalLink {
    pub map_authored: String,
    pub portal_id: String,
}

#[must_use]
pub fn portal_runtime_authored(map_authored: &str, portal_id: &str) -> String {
    format!(
        "portal.{}.{}",
        map_authored.replace('.', "_"),
        portal_id.replace('.', "_")
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Stable editor identity. Unique within one map placement document.
    pub id: String,
    pub kind: PlacementKind,
    /// Authored content reference resolved according to `kind`.
    pub content_authored: String,
    pub position: [f32; 2],
    pub portal_link: Option<PortalLink>,
}
