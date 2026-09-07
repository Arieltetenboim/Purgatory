//! Map instantiation and address transition. JSON-free; consumes typed spawn plans.

use crate::bounds::WorldBounds;
use crate::entity::EntityId;
use crate::entity::EntityKind;
use crate::platform::Platform;
use crate::spawn::RuntimeSpawnRequest;
use crate::transform::Transform;
use crate::world::World;
use purgatory_common::{ContentId, WorldAddress};

/// Record of a live map instance inside a [`World`].
#[derive(Clone, Debug)]
pub struct InstantiatedMap {
    pub address: WorldAddress,
    pub map_content: ContentId,
    pub entity_ids: Vec<EntityId>,
    pub bounds: WorldBounds,
}

/// Typed, already-validated spawn plan. Built by the content crate, never from JSON here.
#[derive(Clone, Debug)]
pub struct MapRuntimePlan {
    pub address: WorldAddress,
    pub map_content: ContentId,
    pub bounds: WorldBounds,
    pub platforms: Vec<PlanPlatform>,
    pub placements: Vec<RuntimeSpawnRequest>,
}

#[derive(Clone, Debug)]
pub struct PlanPlatform {
    pub position: [f32; 2],
    pub platform: Platform,
    pub content_id: Option<ContentId>,
}

/// Why map instantiate failed. Preflight runs before any mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstantiateError {
    AlreadyInstantiated,
    EmptyPlan,
    NonFinite,
    InvalidExtent,
    SpawnFailed,
}

impl World {
    #[must_use]
    pub fn map_instantiated(&self, address: WorldAddress) -> bool {
        self.instantiated.contains_key(&address)
    }

    #[must_use]
    pub fn instantiated_map(&self, address: WorldAddress) -> Option<&InstantiatedMap> {
        self.instantiated.get(&address)
    }

    #[must_use]
    pub fn instantiated_count(&self) -> usize {
        self.instantiated.len()
    }

    /// Preflight, then spawn. On spawn failure, despawn the batch (rollback).
    pub fn instantiate_map(
        &mut self,
        plan: &MapRuntimePlan,
    ) -> Result<InstantiatedMap, InstantiateError> {
        if self.instantiated.contains_key(&plan.address) {
            return Err(InstantiateError::AlreadyInstantiated);
        }
        preflight(plan)?;
        let mut spawned = Vec::new();
        for (index, spec) in plan.platforms.iter().enumerate() {
            let mut platform = spec.platform;
            if platform.support_id == 0 {
                platform.support_id = u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX);
            }
            let mut req = RuntimeSpawnRequest::transient_at(plan.address)
                .with_transform(Transform::from_position(spec.position))
                .with_platform(platform);
            if let Some(id) = spec.content_id {
                req = req.with_content(id);
            }
            match self.spawn(req) {
                Some(id) => spawned.push(id),
                None => {
                    rollback(self, &spawned);
                    return Err(InstantiateError::SpawnFailed);
                }
            }
        }
        for placement in &plan.placements {
            let mut req = placement.clone();
            req.address = plan.address;
            match self.spawn(req) {
                Some(id) => spawned.push(id),
                None => {
                    rollback(self, &spawned);
                    return Err(InstantiateError::SpawnFailed);
                }
            }
        }
        if self.instantiated.is_empty() {
            self.set_bounds(plan.bounds);
        }
        let record = InstantiatedMap {
            address: plan.address,
            map_content: plan.map_content,
            entity_ids: spawned,
            bounds: plan.bounds,
        };
        self.instantiated.insert(plan.address, record.clone());
        Ok(record)
    }

    /// Lazy create: instantiate if missing.
    pub fn ensure_map(
        &mut self,
        plan: &MapRuntimePlan,
    ) -> Result<InstantiatedMap, InstantiateError> {
        if let Some(existing) = self.instantiated.get(&plan.address) {
            return Ok(existing.clone());
        }
        self.instantiate_map(plan)
    }

    /// Destroy authored map entities at `address`. Players are retained.
    pub fn destroy_map(&mut self, address: WorldAddress) -> usize {
        let Some(record) = self.instantiated.remove(&address) else {
            return 0;
        };
        let mut n = 0;
        for id in record.entity_ids {
            if self.kind(id) == Some(EntityKind::Player) {
                continue;
            }
            if self.despawn(id) {
                n += 1;
            }
        }
        n
    }

    /// Move a live entity to another address and pose. Does not despawn.
    pub fn transition_entity(
        &mut self,
        id: EntityId,
        dest: WorldAddress,
        position: [f32; 2],
    ) -> bool {
        if !self.set_address(id, dest) {
            return false;
        }
        let previous = if let Some((transform, player)) = self.player_parts_mut_for(id) {
            let previous = transform.position;
            transform.position = position;
            player.grounded = false;
            player.grounded_on = None;
            player.ignored_platform = None;
            player.velocity = [0.0, 0.0];
            player.coyote_ticks = 0;
            player.jump_buffer_ticks = 0;
            player.jump_active = false;
            Some(previous)
        } else {
            None
        };
        if let Some(previous) = previous {
            self.refresh_spatial(id, previous);
        } else {
            let _ = self.set_transform_position(id, position);
        }
        true
    }
}

fn preflight(plan: &MapRuntimePlan) -> Result<(), InstantiateError> {
    if plan.platforms.is_empty() && plan.placements.is_empty() {
        return Err(InstantiateError::EmptyPlan);
    }
    if !finite_bounds(plan.bounds) {
        return Err(InstantiateError::NonFinite);
    }
    for spec in &plan.platforms {
        if !spec.position[0].is_finite() || !spec.position[1].is_finite() {
            return Err(InstantiateError::NonFinite);
        }
        if spec.platform.half_extents[0] <= 0.0 || spec.platform.half_extents[1] <= 0.0 {
            return Err(InstantiateError::InvalidExtent);
        }
        if !spec.platform.half_extents[0].is_finite() || !spec.platform.half_extents[1].is_finite()
        {
            return Err(InstantiateError::NonFinite);
        }
    }
    for placement in &plan.placements {
        let Some(transform) = placement.transform else {
            return Err(InstantiateError::NonFinite);
        };
        if !transform.position[0].is_finite() || !transform.position[1].is_finite() {
            return Err(InstantiateError::NonFinite);
        }
    }
    Ok(())
}

fn finite_bounds(bounds: WorldBounds) -> bool {
    bounds.min_x.is_finite()
        && bounds.max_x.is_finite()
        && bounds.min_y.is_finite()
        && bounds.max_y.is_finite()
        && bounds.max_x > bounds.min_x
        && bounds.max_y > bounds.min_y
}

fn rollback(world: &mut World, spawned: &[EntityId]) {
    for &id in spawned.iter().rev() {
        let _ = world.despawn(id);
    }
}
