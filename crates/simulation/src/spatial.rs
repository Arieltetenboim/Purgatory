//! Uniform grid spatial index per live [`WorldAddress`].
//!
//! Cell size is configuration, not an architectural invariant. Initial
//! development value: [`SPATIAL_CELL_SIZE_WU`].

use std::collections::{HashMap, HashSet};

use crate::aabb::Aabb;
use crate::entity::EntityId;
use purgatory_common::WorldAddress;

/// Initial uniform-grid cell size in world units. Tunable; not an invariant.
pub const SPATIAL_CELL_SIZE_WU: f32 = 4.0;

/// Integer cell coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
}

/// Map a world position to a cell for `cell_size`.
#[must_use]
pub fn cell_of(position: [f32; 2], cell_size: f32) -> CellCoord {
    debug_assert!(cell_size > 0.0);
    CellCoord {
        x: (position[0] / cell_size).floor() as i32,
        y: (position[1] / cell_size).floor() as i32,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct Membership {
    address: WorldAddress,
    cell: CellCoord,
}

/// Per-address uniform grid. Stores [`EntityId`] only.
pub struct SpatialIndex {
    cell_size: f32,
    cells: HashMap<(WorldAddress, CellCoord), Vec<EntityId>>,
    membership: HashMap<EntityId, Membership>,
}

impl SpatialIndex {
    #[must_use]
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(0.0001),
            cells: HashMap::new(),
            membership: HashMap::new(),
        }
    }

    #[must_use]
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    #[must_use]
    pub fn cell_of_pos(&self, position: [f32; 2]) -> CellCoord {
        cell_of(position, self.cell_size)
    }

    pub fn insert(&mut self, id: EntityId, address: WorldAddress, position: [f32; 2]) {
        self.remove(id);
        let cell = self.cell_of_pos(position);
        self.cells.entry((address, cell)).or_default().push(id);
        self.membership.insert(id, Membership { address, cell });
    }

    pub fn remove(&mut self, id: EntityId) {
        let Some(prev) = self.membership.remove(&id) else {
            return;
        };
        if let Some(list) = self.cells.get_mut(&(prev.address, prev.cell)) {
            list.retain(|&existing| existing != id);
            if list.is_empty() {
                self.cells.remove(&(prev.address, prev.cell));
            }
        }
    }

    /// Move an entity to a (possibly new) address and position.
    pub fn relocate(&mut self, id: EntityId, address: WorldAddress, position: [f32; 2]) {
        let cell = self.cell_of_pos(position);
        if let Some(prev) = self.membership.get(&id)
            && prev.address == address
            && prev.cell == cell
        {
            return;
        }
        self.insert(id, address, position);
    }

    #[must_use]
    pub fn contains(&self, id: EntityId, address: WorldAddress, position: [f32; 2]) -> bool {
        self.membership
            .get(&id)
            .is_some_and(|m| m.address == address && m.cell == self.cell_of_pos(position))
    }

    #[must_use]
    pub fn query_aabb(&self, address: WorldAddress, aabb: Aabb) -> Vec<EntityId> {
        self.query_aabb_inner(address, aabb, true)
    }

    /// Cell-overlap candidates without the deterministic sort (caller may filter/sort).
    #[must_use]
    pub fn query_aabb_unsorted(&self, address: WorldAddress, aabb: Aabb) -> Vec<EntityId> {
        self.query_aabb_inner(address, aabb, false)
    }

    fn query_aabb_inner(&self, address: WorldAddress, aabb: Aabb, sorted: bool) -> Vec<EntityId> {
        let min = self.cell_of_pos([aabb.min_x(), aabb.min_y()]);
        let max = self.cell_of_pos([aabb.max_x(), aabb.max_y()]);
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for x in min.x..=max.x {
            for y in min.y..=max.y {
                let cell = CellCoord { x, y };
                let Some(list) = self.cells.get(&(address, cell)) else {
                    continue;
                };
                for &id in list {
                    if seen.insert(id) {
                        out.push(id);
                    }
                }
            }
        }
        if sorted {
            out.sort_by_key(|id| (id.index(), id.generation()));
        }
        out
    }

    #[must_use]
    pub fn query_point(&self, address: WorldAddress, position: [f32; 2]) -> Vec<EntityId> {
        let cell = self.cell_of_pos(position);
        let mut out = self
            .cells
            .get(&(address, cell))
            .cloned()
            .unwrap_or_default();
        out.sort_by_key(|id| (id.index(), id.generation()));
        out
    }

    #[must_use]
    pub fn query_radius(
        &self,
        address: WorldAddress,
        position: [f32; 2],
        radius: f32,
    ) -> Vec<EntityId> {
        let r = radius.max(0.0);
        let aabb = Aabb::new(position, [r, r]);
        let r2 = r * r;
        self.query_aabb_unsorted(address, aabb)
            .into_iter()
            .filter(|&id| {
                // Distance filter is applied by World, which has Transform.
                // Index returns cell-overlap candidates; World narrows.
                let _ = (id, r2);
                true
            })
            .collect()
    }
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self::new(SPATIAL_CELL_SIZE_WU)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityId;

    fn id(index: u32) -> EntityId {
        EntityId::from_raw(index, 1)
    }

    #[test]
    fn cell_size_is_initial_tunable() {
        assert!((SPATIAL_CELL_SIZE_WU - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn relocate_across_cell_updates_membership() {
        let mut grid = SpatialIndex::new(4.0);
        let a = WorldAddress::DEV;
        let e = id(1);
        grid.insert(e, a, [0.5, 0.5]);
        assert!(grid.contains(e, a, [0.5, 0.5]));
        grid.relocate(e, a, [20.0, 0.5]);
        assert!(
            !grid
                .query_aabb(a, Aabb::new([0.5, 0.5], [1.0, 1.0]))
                .contains(&e)
        );
        assert!(grid.contains(e, a, [20.0, 0.5]));
        assert!(
            grid.query_aabb(a, Aabb::new([20.0, 0.5], [1.0, 1.0]))
                .contains(&e)
        );
    }

    #[test]
    fn query_is_deterministic() {
        let mut grid = SpatialIndex::new(4.0);
        let a = WorldAddress::DEV;
        grid.insert(id(3), a, [0.0, 0.0]);
        grid.insert(id(1), a, [0.1, 0.0]);
        grid.insert(id(2), a, [0.2, 0.0]);
        let found = grid.query_aabb(a, Aabb::new([0.0, 0.0], [2.0, 2.0]));
        assert_eq!(found, vec![id(1), id(2), id(3)]);
    }
}
