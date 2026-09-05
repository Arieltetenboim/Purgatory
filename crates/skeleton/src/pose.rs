//! Caller-owned local and world pose buffers. Evaluate does not allocate.

use crate::def::SkeletonDef;
use crate::xform::{BoneIndex, BoneTransform, SlotIndex};

/// Why evaluate or slot lookup failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseError {
    BoneCountMismatch {
        definition: usize,
        local: usize,
        world: usize,
    },
    SlotOutOfRange {
        slot: u8,
        slot_count: usize,
    },
}

/// Final local translation + rotation of each bone relative to its parent.
///
/// This is not an animation-delta buffer. Bind pose is a copy of the definition's
/// bind locals. Future clip code may write this buffer; the skeleton core does not
/// know about clips, additive layers, or gameplay state.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalPose {
    bones: Box<[BoneTransform]>,
}

impl LocalPose {
    /// Copy definition bind locals into a new caller-owned buffer.
    #[must_use]
    pub fn from_bind(def: &SkeletonDef) -> Self {
        Self {
            bones: def.bind_locals().to_vec().into_boxed_slice(),
        }
    }

    /// Identity locals; length is independent of any definition until evaluate.
    #[must_use]
    pub fn with_identity(bone_count: usize) -> Self {
        Self {
            bones: vec![BoneTransform::IDENTITY; bone_count].into_boxed_slice(),
        }
    }

    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    #[must_use]
    pub fn get(&self, bone: BoneIndex) -> Option<BoneTransform> {
        self.bones.get(bone.as_usize()).copied()
    }

    pub fn get_mut(&mut self, bone: BoneIndex) -> Option<&mut BoneTransform> {
        self.bones.get_mut(bone.as_usize())
    }

    /// Replace locals with the definition bind pose. Does not allocate when counts match.
    pub fn copy_bind(&mut self, def: &SkeletonDef) -> Result<(), PoseError> {
        if self.bones.len() != def.bone_count() {
            return Err(PoseError::BoneCountMismatch {
                definition: def.bone_count(),
                local: self.bones.len(),
                world: 0,
            });
        }
        self.bones.copy_from_slice(def.bind_locals());
        Ok(())
    }

    #[must_use]
    pub fn as_slice(&self) -> &[BoneTransform] {
        &self.bones
    }

    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [BoneTransform] {
        &mut self.bones
    }

    /// Copy bone transforms from `other`. Does not allocate when counts match.
    pub fn copy_from(&mut self, other: &LocalPose) -> Result<(), PoseError> {
        if self.bones.len() != other.bones.len() {
            return Err(PoseError::BoneCountMismatch {
                definition: other.bones.len(),
                local: self.bones.len(),
                world: 0,
            });
        }
        self.bones.copy_from_slice(&other.bones);
        Ok(())
    }
}

/// Derived world-space transforms. Caller allocates; evaluate only writes this buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldPose {
    bones: Box<[BoneTransform]>,
}

impl WorldPose {
    #[must_use]
    pub fn new(def: &SkeletonDef) -> Self {
        Self::with_bone_count(def.bone_count())
    }

    #[must_use]
    pub fn with_bone_count(bone_count: usize) -> Self {
        Self {
            bones: vec![BoneTransform::IDENTITY; bone_count].into_boxed_slice(),
        }
    }

    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    #[must_use]
    pub fn get(&self, bone: BoneIndex) -> Option<BoneTransform> {
        self.bones.get(bone.as_usize()).copied()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[BoneTransform] {
        &self.bones
    }
}

/// `Definition + local Pose → world Pose`.
///
/// Bone counts on `def`, `local`, and `world` must match. This is checked before the
/// `O(bone_count)` write loop so evaluate never relies on accidental short-slice indexing.
///
/// The hot path writes only into `world` and reads `def` / `local`. It does not allocate,
/// grow buffers, or build temporary collections. Buffers must be allocated by the caller
/// (`LocalPose::from_bind`, `WorldPose::new`, or equivalent) before calling.
pub fn evaluate(
    def: &SkeletonDef,
    local: &LocalPose,
    world: &mut WorldPose,
) -> Result<(), PoseError> {
    let n = def.bone_count();
    if local.bone_count() != n || world.bone_count() != n {
        return Err(PoseError::BoneCountMismatch {
            definition: n,
            local: local.bone_count(),
            world: world.bone_count(),
        });
    }
    evaluate_equal_len(def, local.as_slice(), &mut world.bones);
    Ok(())
}

fn evaluate_equal_len(def: &SkeletonDef, local: &[BoneTransform], world: &mut [BoneTransform]) {
    let parents = def.parents();
    for i in 0..local.len() {
        let local_t = local[i];
        world[i] = match parents[i] {
            None => local_t,
            Some(parent) => world[parent.as_usize()].compose(local_t),
        };
    }
}

/// Slot rest composed onto the parent bone's world transform.
pub fn slot_world(
    def: &SkeletonDef,
    world: &WorldPose,
    slot: SlotIndex,
) -> Result<BoneTransform, PoseError> {
    if world.bone_count() != def.bone_count() {
        return Err(PoseError::BoneCountMismatch {
            definition: def.bone_count(),
            local: def.bone_count(),
            world: world.bone_count(),
        });
    }
    let Some(slot_def) = def.slot(slot) else {
        return Err(PoseError::SlotOutOfRange {
            slot: slot.as_u8(),
            slot_count: def.slot_count(),
        });
    };
    let bone_world = world
        .get(slot_def.bone)
        .expect("validated slot bone is in range when world count matches def");
    Ok(bone_world.compose(slot_def.rest))
}
