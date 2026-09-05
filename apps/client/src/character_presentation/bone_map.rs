//! Bind-time `content::BoneTarget` → Humanoid v0 `BoneIndex`.
//!
//! Content does not import skeleton. Mapping lives at this presentation boundary.
//! Unknown bones fail; they are never silently remapped to Root/Pelvis.

use purgatory_content::BoneTarget;
use purgatory_skeleton::{BoneIndex, humanoid_v0_bone_by_label};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissingCanonicalBone {
    pub label: &'static str,
}

impl std::fmt::Display for MissingCanonicalBone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "canonical BoneTarget '{}' is missing from the humanoid rig",
            self.label
        )
    }
}

/// Static per-rig table. Resolve once at bind, not per frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoneTargetMap {
    indices: [BoneIndex; BoneTarget::ALL.len()],
}

impl BoneTargetMap {
    pub fn bind_humanoid_v0() -> Result<Self, MissingCanonicalBone> {
        Self::bind_with_lookup(humanoid_v0_bone_by_label)
    }

    pub fn bind_with_lookup(
        lookup: fn(&str) -> Option<BoneIndex>,
    ) -> Result<Self, MissingCanonicalBone> {
        let mut indices = [BoneIndex::from_u8(0); BoneTarget::ALL.len()];
        for (i, target) in BoneTarget::ALL.iter().copied().enumerate() {
            let label = target.as_str();
            let Some(index) = lookup(label) else {
                return Err(MissingCanonicalBone { label });
            };
            indices[i] = index;
        }
        Ok(Self { indices })
    }

    #[must_use]
    pub fn bone(self, target: BoneTarget) -> BoneIndex {
        self.indices[target_index(target)]
    }
}

/// Canonical `BoneTarget::ALL` order. Bit `i` is `ALL[i]`.
#[must_use]
pub const fn hide_bit(target: BoneTarget) -> u16 {
    1 << target_index(target)
}

#[must_use]
pub fn hide_mask(targets: impl IntoIterator<Item = BoneTarget>) -> u16 {
    targets
        .into_iter()
        .fold(0, |acc, target| acc | hide_bit(target))
}

#[must_use]
pub const fn hide_contains(mask: u16, target: BoneTarget) -> bool {
    mask & hide_bit(target) != 0
}

const fn target_index(target: BoneTarget) -> usize {
    match target {
        BoneTarget::Head => 0,
        BoneTarget::Torso => 1,
        BoneTarget::UpperArmFront => 2,
        BoneTarget::LowerArmFront => 3,
        BoneTarget::HandFront => 4,
        BoneTarget::UpperArmBack => 5,
        BoneTarget::LowerArmBack => 6,
        BoneTarget::HandBack => 7,
        BoneTarget::UpperLegFront => 8,
        BoneTarget::LowerLegFront => 9,
        BoneTarget::FootFront => 10,
        BoneTarget::UpperLegBack => 11,
        BoneTarget::LowerLegBack => 12,
        BoneTarget::FootBack => 13,
    }
}

#[cfg(test)]
mod index_contract {
    use super::{hide_bit, hide_contains, hide_mask, target_index};
    use purgatory_content::BoneTarget;

    #[test]
    fn all_order_matches_target_index() {
        for (i, target) in BoneTarget::ALL.iter().copied().enumerate() {
            assert_eq!(target_index(target), i);
        }
    }

    #[test]
    fn hide_bit_is_locked_to_all_order() {
        assert_eq!(BoneTarget::ALL.len(), 14);
        let mut seen = 0u16;
        for (i, target) in BoneTarget::ALL.iter().copied().enumerate() {
            let bit = hide_bit(target);
            assert_eq!(bit, 1u16 << i);
            assert_eq!(bit.trailing_zeros() as usize, i);
            assert_eq!(bit.count_ones(), 1);
            assert_eq!(seen & bit, 0, "duplicate hide bit for {target:?}");
            seen |= bit;
            assert!(hide_contains(bit, target));
            assert!(!hide_contains(0, target));
        }
        assert_eq!(seen, (1u16 << 14) - 1);
        assert_eq!(
            hide_mask([BoneTarget::Torso, BoneTarget::Head]),
            hide_bit(BoneTarget::Torso) | hide_bit(BoneTarget::Head)
        );
    }
}
