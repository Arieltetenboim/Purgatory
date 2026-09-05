//! 2D rigid transform: translation + one plane rotation. No scale, no matrices.

/// Dense bone index within one [`crate::SkeletonDef`]. Not a closed named-bone enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BoneIndex(u8);

impl BoneIndex {
    #[must_use]
    pub const fn from_u8(index: u8) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Dense slot index within one [`crate::SkeletonDef`]. Independent of bone count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotIndex(u8);

impl SlotIndex {
    #[must_use]
    pub const fn from_u8(index: u8) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Local or world 2D rigid transform. Rotation is radians, counterclockwise, `+Y` up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneTransform {
    pub translation: [f32; 2],
    pub rotation: f32,
}

impl BoneTransform {
    pub const IDENTITY: Self = Self {
        translation: [0.0, 0.0],
        rotation: 0.0,
    };

    #[must_use]
    pub const fn from_translation_rotation(translation: [f32; 2], rotation: f32) -> Self {
        Self {
            translation,
            rotation,
        }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.translation[0].is_finite()
            && self.translation[1].is_finite()
            && self.rotation.is_finite()
    }

    /// `parent ∘ local`: child in the parent's space.
    #[must_use]
    pub fn compose(self, local: Self) -> Self {
        let cos = self.rotation.cos();
        let sin = self.rotation.sin();
        let lx = local.translation[0];
        let ly = local.translation[1];
        Self {
            translation: [
                self.translation[0] + cos * lx - sin * ly,
                self.translation[1] + sin * lx + cos * ly,
            ],
            rotation: self.rotation + local.rotation,
        }
    }
}
