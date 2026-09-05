//! Phase 8F-A draw-order contract. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{
    BoneTarget, ContentRegistry, LoadMode, default_content_root, load_registry,
};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;

use super::adapters::from_local;
use super::bone_map::{BoneTargetMap, hide_bit};
use super::debug_visual::presentation_debug_quads;
use super::draw_order::{
    BasePiece, PlannedKind, PresentationLayer, layer_for_attachment, layer_for_bone_target,
    plan_character_draw,
};
use super::resolve::{BoundAttachment, resolve_equipment};
use super::state::{EquipmentView, Facing, PresentationView};
use super::{CharacterPresentationSet, LocalMotion, PresentationEntityKey};

fn pack() -> ContentRegistry {
    load_registry(&default_content_root(), LoadMode::Shared).expect("shared content")
}

fn cid(authored: &str) -> ContentId {
    ContentId::from_authored(authored).expect("authored id")
}

fn present_slots(pairs: &[(EquipmentSlot, ContentId)]) -> EquipmentView {
    let mut slots = [None; EquipmentSlot::COUNT];
    for (slot, id) in pairs {
        slots[slot.index()] = Some(*id);
    }
    EquipmentView::present(slots)
}

fn idle(equipment: EquipmentView) -> super::state::CharacterPresentationState {
    from_local(
        LocalMotion {
            pose: [0.0, 0.0],
            velocity: [0.0, 0.0],
            grounded: true,
            equipment,
        },
        Facing::Right,
    )
}

fn base_sequence(plan: &[PlannedKind]) -> Vec<BasePiece> {
    plan.iter()
        .filter_map(|kind| match kind {
            PlannedKind::Base(piece) => Some(*piece),
            PlannedKind::Attachment(_) => None,
        })
        .collect()
}

fn attachment_ids(plan: &[PlannedKind], bound: &[BoundAttachment]) -> Vec<String> {
    plan.iter()
        .filter_map(|kind| match kind {
            PlannedKind::Attachment(i) => bound.get(*i).map(|b| b.attachment_id.clone()),
            PlannedKind::Base(_) => None,
        })
        .collect()
}

fn plan_side(hidden_base: u16, attachments: &[BoundAttachment]) -> Vec<PlannedKind> {
    plan_character_draw(hidden_base, attachments, PresentationView::Side)
}

#[test]
fn protocol_unchanged() {
    assert_eq!(PROTOCOL_VERSION, 15);
}

#[test]
fn canonical_layers_are_strictly_far_to_near() {
    let all = PresentationLayer::ALL;
    for pair in all.windows(2) {
        assert!(pair[0] < pair[1]);
        assert!(pair[0].as_u8() < pair[1].as_u8());
    }
    assert_eq!(all[0], PresentationLayer::ArmBack);
    assert_eq!(all[1], PresentationLayer::LegBack);
    assert_eq!(all[2], PresentationLayer::Core);
    assert_eq!(all[3], PresentationLayer::LegFront);
    assert_eq!(all[4], PresentationLayer::Head);
    assert_eq!(all[5], PresentationLayer::ArmFront);
}

#[test]
fn bone_targets_map_to_semantic_layers() {
    assert_eq!(
        layer_for_bone_target(BoneTarget::HandBack),
        PresentationLayer::ArmBack
    );
    assert_eq!(
        layer_for_bone_target(BoneTarget::FootBack),
        PresentationLayer::LegBack
    );
    assert_eq!(
        layer_for_bone_target(BoneTarget::Torso),
        PresentationLayer::Core
    );
    assert_eq!(
        layer_for_bone_target(BoneTarget::FootFront),
        PresentationLayer::LegFront
    );
    assert_eq!(
        layer_for_bone_target(BoneTarget::Head),
        PresentationLayer::Head
    );
    assert_eq!(
        layer_for_bone_target(BoneTarget::HandFront),
        PresentationLayer::ArmFront
    );
}

#[test]
fn attachments_follow_authored_bone_layer() {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    let sword = resolve_equipment(
        present_slots(&[(EquipmentSlot::Weapon, cid("equipment.debug.practice_sword"))]),
        &registry,
        map,
    );
    assert_eq!(sword.bound.len(), 1);
    assert_eq!(
        layer_for_attachment(&sword.bound[0]),
        PresentationLayer::ArmFront
    );
    let boots = resolve_equipment(
        present_slots(&[(EquipmentSlot::Boots, cid("equipment.debug.iron_boots"))]),
        &registry,
        map,
    );
    assert_eq!(boots.bound.len(), 2);
    let layers: Vec<_> = boots.bound.iter().map(layer_for_attachment).collect();
    assert!(layers.contains(&PresentationLayer::LegFront));
    assert!(layers.contains(&PresentationLayer::LegBack));
}

#[test]
fn plan_is_stable_when_attachment_slice_is_reversed() {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    let out = resolve_equipment(
        present_slots(&[
            (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
            (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
            (EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap")),
        ]),
        &registry,
        map,
    );
    let mut reversed = out.bound.clone();
    reversed.reverse();
    let a = plan_side(0, &out.bound);
    let b = plan_side(0, &reversed);
    assert_eq!(base_sequence(&a), base_sequence(&b));
    assert_eq!(
        attachment_ids(&a, &out.bound),
        attachment_ids(&b, &reversed)
    );
}

#[test]
fn hidden_base_omits_only_that_bone_and_keeps_remaining_order() {
    let full = base_sequence(&plan_side(0, &[]));
    assert_eq!(full, BasePiece::ALL.to_vec());
    let hidden = hide_bit(BoneTarget::Torso);
    let trimmed = base_sequence(&plan_side(hidden, &[]));
    let expected: Vec<_> = BasePiece::ALL
        .into_iter()
        .filter(|p| p.hide_target() != BoneTarget::Torso)
        .collect();
    assert_eq!(trimmed, expected);
    assert!(!trimmed.contains(&BasePiece::TorsoFar));
    assert!(!trimmed.contains(&BasePiece::TorsoNear));
    let head_i = trimmed.iter().position(|p| *p == BasePiece::Head).unwrap();
    let arm_i = trimmed
        .iter()
        .position(|p| *p == BasePiece::UpperArmFront)
        .unwrap();
    let back_arm_i = trimmed
        .iter()
        .position(|p| *p == BasePiece::UpperArmBack)
        .unwrap();
    assert!(back_arm_i < head_i);
    assert!(head_i < arm_i);
}

#[test]
fn sword_emits_in_arm_front_after_core_and_back_limbs() {
    let registry = pack();
    let key = PresentationEntityKey::new(1, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(
            key,
            idle(present_slots(&[(
                EquipmentSlot::Weapon,
                cid("equipment.debug.practice_sword"),
            )])),
        )],
        &registry,
        0.0,
    );
    let entry = set.get(key).unwrap();
    let plan = plan_side(entry.hidden_base(), entry.bound());
    let sword_i = plan
        .iter()
        .position(|k| matches!(k, PlannedKind::Attachment(_)))
        .unwrap();
    let torso_i = plan
        .iter()
        .position(|k| matches!(k, PlannedKind::Base(BasePiece::TorsoNear)))
        .unwrap();
    let back_hand_i = plan
        .iter()
        .position(|k| matches!(k, PlannedKind::Base(BasePiece::HandBack)))
        .unwrap();
    let front_hand_i = plan
        .iter()
        .position(|k| matches!(k, PlannedKind::Base(BasePiece::HandFront)))
        .unwrap();
    assert!(back_hand_i < torso_i);
    assert!(torso_i < front_hand_i);
    assert!(front_hand_i < sword_i);
    let quads = presentation_debug_quads(set.bone_map(), entry, 1.15, true, PresentationView::Side);
    assert_eq!(quads.len(), plan.len());
}

#[test]
fn local_and_remote_share_draw_plan() {
    let registry = pack();
    let equipment = present_slots(&[
        (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
        (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
    ]);
    let mut set = CharacterPresentationSet::new();
    let local_key = PresentationEntityKey::new(3, 1);
    let remote_key = PresentationEntityKey::new(9, 2);
    set.sync(
        [
            (local_key, idle(equipment)),
            (
                remote_key,
                super::adapters::from_remote(
                    super::RemoteMotion {
                        pose: [4.0, 0.0],
                        velocity: [0.0, 0.0],
                        equipment,
                    },
                    Facing::Right,
                ),
            ),
        ],
        &registry,
        0.0,
    );
    let local = set.get(local_key).unwrap();
    let remote = set.get(remote_key).unwrap();
    let lp = plan_side(local.hidden_base(), local.bound());
    let rp = plan_side(remote.hidden_base(), remote.bound());
    assert_eq!(base_sequence(&lp), base_sequence(&rp));
    assert_eq!(
        attachment_ids(&lp, local.bound()),
        attachment_ids(&rp, remote.bound())
    );
    let keys: Vec<_> = set.iter_draw_order().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![local_key, remote_key]);
}
