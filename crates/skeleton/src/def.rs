//! Immutable skeleton definition and construction-time validation.

use crate::xform::{BoneIndex, BoneTransform, SlotIndex};

/// Why a definition was rejected. Construction only; evaluate does not allocate this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkeletonDefError {
    Empty,
    BoneCountMismatch { parents: usize, bind_locals: usize },
    TooManyBones { count: usize },
    TooManySlots { count: usize },
    RootHasParent,
    ExtraRoot { bone: u8 },
    ParentOutOfRange { bone: u8, parent: u8 },
    ParentNotStrictlyLess { bone: u8, parent: u8 },
    SlotBoneOutOfRange { slot: u8, bone: u8 },
    DrawOrderLength { expected: usize, got: usize },
    DrawOrderOutOfRange { slot: u8 },
    DrawOrderDuplicate { slot: u8 },
    NonFiniteBind { bone: u8 },
    NonFiniteSlotRest { slot: u8 },
}

/// Slot metadata: parent bone + rest local transform (translation + rotation).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotDef {
    pub bone: BoneIndex,
    pub rest: BoneTransform,
}

/// Shared immutable hierarchy, bind locals, and slot table.
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonDef {
    parents: Box<[Option<BoneIndex>]>,
    bind_locals: Box<[BoneTransform]>,
    slots: Box<[SlotDef]>,
    draw_order: Box<[SlotIndex]>,
}

impl SkeletonDef {
    /// Validate and take ownership of definition arrays.
    ///
    /// Invariants: exactly one root at index 0; every other bone has exactly one parent
    /// whose index is strictly less than the child; all transforms are finite; draw order
    /// is a permutation of slot indices (independent of hierarchy).
    pub fn try_new(
        parents: Vec<Option<BoneIndex>>,
        bind_locals: Vec<BoneTransform>,
        slots: Vec<SlotDef>,
        draw_order: Vec<SlotIndex>,
    ) -> Result<Self, SkeletonDefError> {
        if parents.is_empty() {
            return Err(SkeletonDefError::Empty);
        }
        if parents.len() != bind_locals.len() {
            return Err(SkeletonDefError::BoneCountMismatch {
                parents: parents.len(),
                bind_locals: bind_locals.len(),
            });
        }
        if parents.len() > u8::MAX as usize + 1 {
            return Err(SkeletonDefError::TooManyBones {
                count: parents.len(),
            });
        }
        if slots.len() > u8::MAX as usize + 1 {
            return Err(SkeletonDefError::TooManySlots { count: slots.len() });
        }

        let bone_count = parents.len();
        if parents[0].is_some() {
            return Err(SkeletonDefError::RootHasParent);
        }
        if !bind_locals[0].is_finite() {
            return Err(SkeletonDefError::NonFiniteBind { bone: 0 });
        }

        for (i, parent) in parents.iter().enumerate().skip(1) {
            let bone = u8::try_from(i).expect("bone_count fits u8");
            let Some(p) = *parent else {
                return Err(SkeletonDefError::ExtraRoot { bone });
            };
            if p.as_usize() >= bone_count {
                return Err(SkeletonDefError::ParentOutOfRange {
                    bone,
                    parent: p.as_u8(),
                });
            }
            if p.as_usize() >= i {
                return Err(SkeletonDefError::ParentNotStrictlyLess {
                    bone,
                    parent: p.as_u8(),
                });
            }
            if !bind_locals[i].is_finite() {
                return Err(SkeletonDefError::NonFiniteBind { bone });
            }
        }

        for (si, slot) in slots.iter().enumerate() {
            let slot_i = u8::try_from(si).expect("slot_count fits u8");
            if slot.bone.as_usize() >= bone_count {
                return Err(SkeletonDefError::SlotBoneOutOfRange {
                    slot: slot_i,
                    bone: slot.bone.as_u8(),
                });
            }
            if !slot.rest.is_finite() {
                return Err(SkeletonDefError::NonFiniteSlotRest { slot: slot_i });
            }
        }

        if draw_order.len() != slots.len() {
            return Err(SkeletonDefError::DrawOrderLength {
                expected: slots.len(),
                got: draw_order.len(),
            });
        }
        let mut seen = [false; 256];
        for &idx in &draw_order {
            if idx.as_usize() >= slots.len() {
                return Err(SkeletonDefError::DrawOrderOutOfRange { slot: idx.as_u8() });
            }
            if seen[idx.as_usize()] {
                return Err(SkeletonDefError::DrawOrderDuplicate { slot: idx.as_u8() });
            }
            seen[idx.as_usize()] = true;
        }

        Ok(Self {
            parents: parents.into_boxed_slice(),
            bind_locals: bind_locals.into_boxed_slice(),
            slots: slots.into_boxed_slice(),
            draw_order: draw_order.into_boxed_slice(),
        })
    }

    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.parents.len()
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn parent(&self, bone: BoneIndex) -> Option<BoneIndex> {
        self.parents.get(bone.as_usize()).copied().flatten()
    }

    #[must_use]
    pub fn bind_local(&self, bone: BoneIndex) -> Option<BoneTransform> {
        self.bind_locals.get(bone.as_usize()).copied()
    }

    #[must_use]
    pub fn bind_locals(&self) -> &[BoneTransform] {
        &self.bind_locals
    }

    #[must_use]
    pub fn parents(&self) -> &[Option<BoneIndex>] {
        &self.parents
    }

    #[must_use]
    pub fn slot(&self, slot: SlotIndex) -> Option<SlotDef> {
        self.slots.get(slot.as_usize()).copied()
    }

    #[must_use]
    pub fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    /// Slot draw order. Independent of parent hierarchy.
    #[must_use]
    pub fn draw_order(&self) -> &[SlotIndex] {
        &self.draw_order
    }
}
