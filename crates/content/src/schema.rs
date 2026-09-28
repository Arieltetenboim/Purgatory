//! Typed content definitions. Gameplay never sees raw JSON.

use crate::domain::ContentDomain;
use crate::map_gameplay_authoring::FootholdPath;
use purgatory_common::{ContentId, ContentKind, allocated_id_for_label, label_for_allocated_id};
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
    pub restore: Option<RestorePolicy>,
}

impl MapDefinition {
    /// Footnote paths plus a default spawn. Distinct from merely being a known authored map.
    #[must_use]
    pub fn is_gameplay_ready(&self) -> bool {
        !self.foothold_paths.is_empty()
            && self.spawn_points.iter().any(|spawn| spawn.id == "default")
    }
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

/// Why a map-owned portal ContentId cannot be used.
///
/// `None` means `content_id` is the catalog allocation for `{map}.{portal_id}`.
#[must_use]
pub fn portal_identity_error(
    map_authored: &str,
    portal_id: &str,
    content_id: Option<ContentId>,
) -> Option<&'static str> {
    let Some(content_id) = content_id else {
        return Some("portal requires a numeric ContentId");
    };
    if content_id.kind() != Some(ContentKind::WorldObject) {
        return Some("portal ContentId is outside the World Object block");
    }
    let label = format!("{map_authored}.{portal_id}");
    match label_for_allocated_id(content_id) {
        Some(recorded)
            if recorded == label && allocated_id_for_label(&label) == Some(content_id) =>
        {
            None
        }
        Some(_) => Some("portal ContentId is not allocated to this portal"),
        None => Some("unknown portal ContentId"),
    }
}

/// Catalog ContentId for a map-owned portal, when the label is allocated.
#[must_use]
pub fn catalog_portal_content_id(map_authored: &str, portal_id: &str) -> Option<ContentId> {
    let label = format!("{map_authored}.{portal_id}");
    let content_id = allocated_id_for_label(&label)?;
    portal_identity_error(map_authored, portal_id, Some(content_id))
        .is_none()
        .then_some(content_id)
}

/// Accepted portal identity. Unknown, duplicate-label, and wrong-domain IDs are rejected.
#[must_use]
pub fn resolve_portal_content_id(
    map_authored: &str,
    portal_id: &str,
    content_id: ContentId,
) -> Option<ContentId> {
    portal_identity_error(map_authored, portal_id, Some(content_id))
        .is_none()
        .then_some(content_id)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// Stable editor identity. Unique within one map placement document.
    pub id: String,
    pub kind: PlacementKind,
    /// Authored content reference for entity and monster placements.
    ///
    /// Map-owned portals keep this empty. Their canonical identity is [`Placement::content_id`].
    pub content_authored: String,
    /// Numeric World Object identity for a map-owned portal.
    pub content_id: Option<ContentId>,
    pub position: [f32; 2],
    pub portal_link: Option<PortalLink>,
}
