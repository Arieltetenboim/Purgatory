//! Phase 8F-B Front/Back visibility. Same layer table as 8F-A. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{
    BoneTarget, ContentRegistry, LoadMode, default_content_root, load_registry,
};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;

use super::adapters::{from_local, from_remote};
use super::bone_map::{BoneTargetMap, hide_bit};
use super::debug_visual::presentation_debug_quads;
use super::draw_order::{
    BasePiece, PlannedKind, PresentationLayer, layer_for_attachment, plan_character_draw,
};
use super::resolve::{BoundAttachment, resolve_equipment};
use super::state::{
    EquipmentView, Facing, PresentationActivity, PresentationView, view_for_activity,
};
use super::{CharacterPresentationSet, LocalMotion, PresentationEntityKey, RemoteMotion};

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

fn kit() -> Vec<BoundAttachment> {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    resolve_equipment(
        present_slots(&[
            (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
            (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
            (EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap")),
            (EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
        ]),
        &registry,
        map,
    )
    .bound
}

const BACK_BASES: [BasePiece; 9] = [
    BasePiece::TorsoFar,
    BasePiece::TorsoNear,
    BasePiece::UpperLegBack,
    BasePiece::LowerLegBack,
    BasePiece::FootBack,
    BasePiece::Head,
    BasePiece::UpperArmBack,
    BasePiece::LowerArmBack,
    BasePiece::HandBack,
];

#[test]
fn protocol_unchanged() {
    assert_eq!(PROTOCOL_VERSION, 24);
}

#[test]
fn current_activities_remain_side_view() {
    for activity in [
        PresentationActivity::Idle,
        PresentationActivity::Move,
        PresentationActivity::Jump,
        PresentationActivity::Fall,
        PresentationActivity::Attack,
        PresentationActivity::Hurt,
        PresentationActivity::Dead,
    ] {
        assert_eq!(view_for_activity(activity), PresentationView::Side);
    }
    let mut back = idle(EquipmentView::Absent);
    back.view = PresentationView::Back;
    assert!(back.is_placeholder_ready());
}

#[test]
fn side_visibility_and_paint_are_identity() {
    for layer in PresentationLayer::ALL {
        assert!(layer.authored_layer_visible(PresentationView::Side));
        assert_eq!(layer.paint_layer(PresentationView::Side), layer);
    }
}

#[test]
fn back_hides_front_limbs_and_remaps_back_limbs_near() {
    assert!(!PresentationLayer::ArmFront.authored_layer_visible(PresentationView::Back));
    assert!(!PresentationLayer::LegFront.authored_layer_visible(PresentationView::Back));
    assert!(PresentationLayer::ArmBack.authored_layer_visible(PresentationView::Back));
    assert!(PresentationLayer::LegBack.authored_layer_visible(PresentationView::Back));
    assert!(PresentationLayer::Core.authored_layer_visible(PresentationView::Back));
    assert!(PresentationLayer::Head.authored_layer_visible(PresentationView::Back));
    assert_eq!(
        PresentationLayer::ArmBack.paint_layer(PresentationView::Back),
        PresentationLayer::ArmFront
    );
    assert_eq!(
        PresentationLayer::LegBack.paint_layer(PresentationView::Back),
        PresentationLayer::LegFront
    );
    assert_eq!(
        PresentationLayer::Core.paint_layer(PresentationView::Back),
        PresentationLayer::Core
    );
    assert_eq!(
        PresentationLayer::Head.paint_layer(PresentationView::Back),
        PresentationLayer::Head
    );
}

#[test]
fn canonical_layer_order_is_unchanged() {
    assert_eq!(
        PresentationLayer::ALL,
        [
            PresentationLayer::ArmBack,
            PresentationLayer::LegBack,
            PresentationLayer::Core,
            PresentationLayer::LegFront,
            PresentationLayer::Head,
            PresentationLayer::ArmFront,
        ]
    );
}

#[test]
fn side_draw_plan_matches_8f_a() {
    let bound = kit();
    let plan = plan_character_draw(0, &bound, PresentationView::Side);
    assert_eq!(base_sequence(&plan), BasePiece::ALL.to_vec());
    let ids = attachment_ids(&plan, &bound);
    assert!(ids.contains(&"blade".to_string()));
    assert!(ids.contains(&"foot_front".to_string()));
    assert!(ids.contains(&"foot_back".to_string()));
    assert!(ids.contains(&"hand_front".to_string()));
    assert!(ids.contains(&"hand_back".to_string()));
}

#[test]
fn back_base_plan_omits_front_limbs_and_paints_back_limbs_near() {
    let plan = plan_character_draw(0, &[], PresentationView::Back);
    let bases = base_sequence(&plan);
    assert_eq!(bases, BACK_BASES.to_vec());
    assert!(!bases.contains(&BasePiece::UpperArmFront));
    assert!(!bases.contains(&BasePiece::HandFront));
    assert!(!bases.contains(&BasePiece::UpperLegFront));
    assert!(!bases.contains(&BasePiece::FootFront));
    let torso = bases
        .iter()
        .position(|p| *p == BasePiece::TorsoNear)
        .unwrap();
    let back_leg = bases
        .iter()
        .position(|p| *p == BasePiece::UpperLegBack)
        .unwrap();
    let head = bases.iter().position(|p| *p == BasePiece::Head).unwrap();
    let back_arm = bases
        .iter()
        .position(|p| *p == BasePiece::UpperArmBack)
        .unwrap();
    assert!(torso < back_leg);
    assert!(back_leg < head);
    assert!(head < back_arm);
}

#[test]
fn attachments_inherit_bone_layer_visibility_and_paint_slot() {
    let bound = kit();
    let side = plan_character_draw(0, &bound, PresentationView::Side);
    let back = plan_character_draw(0, &bound, PresentationView::Back);
    let side_ids = attachment_ids(&side, &bound);
    let back_ids = attachment_ids(&back, &bound);
    assert!(side_ids.contains(&"blade".to_string()));
    assert!(side_ids.contains(&"hand_front".to_string()));
    assert!(side_ids.contains(&"foot_front".to_string()));
    assert!(!back_ids.contains(&"blade".to_string()));
    assert!(!back_ids.contains(&"hand_front".to_string()));
    assert!(!back_ids.contains(&"foot_front".to_string()));
    assert!(
        !back_ids.contains(&"hand_back".to_string()),
        "Side-only glove must not Side-as-Back"
    );
    assert!(back_ids.contains(&"foot_back".to_string()));
    assert!(back_ids.contains(&"crown".to_string()));

    let sword_layer = bound
        .iter()
        .find(|b| b.attachment_id == "blade")
        .map(layer_for_attachment)
        .unwrap();
    assert_eq!(sword_layer, PresentationLayer::ArmFront);
    assert!(!sword_layer.authored_layer_visible(PresentationView::Back));

    let back_boot_i = back
        .iter()
        .position(|k| match k {
            PlannedKind::Attachment(i) => bound[*i].attachment_id == "foot_back",
            PlannedKind::Base(_) => false,
        })
        .unwrap();
    let foot_back_i = back
        .iter()
        .position(|k| matches!(k, PlannedKind::Base(BasePiece::FootBack)))
        .unwrap();
    let head_i = back
        .iter()
        .position(|k| matches!(k, PlannedKind::Base(BasePiece::Head)))
        .unwrap();
    assert!(foot_back_i < back_boot_i);
    assert!(back_boot_i < head_i);
}

#[test]
fn hidden_base_still_omits_only_matching_bases() {
    let bound = kit();
    let hidden = hide_bit(BoneTarget::Torso);
    let side = base_sequence(&plan_character_draw(hidden, &bound, PresentationView::Side));
    assert!(!side.contains(&BasePiece::TorsoFar));
    assert!(!side.contains(&BasePiece::TorsoNear));
    assert!(side.contains(&BasePiece::UpperArmFront));
    assert!(
        attachment_ids(
            &plan_character_draw(hidden, &bound, PresentationView::Side),
            &bound
        )
        .contains(&"blade".to_string())
    );

    let back = plan_character_draw(hidden, &bound, PresentationView::Back);
    let bases = base_sequence(&back);
    assert!(!bases.contains(&BasePiece::TorsoFar));
    assert!(!bases.contains(&BasePiece::TorsoNear));
    assert_eq!(
        bases,
        vec![
            BasePiece::UpperLegBack,
            BasePiece::LowerLegBack,
            BasePiece::FootBack,
            BasePiece::Head,
            BasePiece::UpperArmBack,
            BasePiece::LowerArmBack,
            BasePiece::HandBack,
        ]
    );
    let ids = attachment_ids(&back, &bound);
    assert!(!ids.contains(&"blade".to_string()));
    assert!(ids.contains(&"foot_back".to_string()));
}

#[test]
fn replace_base_cuirass_hides_authored_upper_arms_in_back_view() {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    let out = resolve_equipment(
        present_slots(&[(
            EquipmentSlot::Bodywear,
            cid("equipment.debug.plate_cuirass"),
        )]),
        &registry,
        map,
    );
    assert!(out.hidden_base != 0);
    let back = plan_character_draw(out.hidden_base, &out.bound, PresentationView::Back);
    let bases = base_sequence(&back);
    assert!(!bases.contains(&BasePiece::TorsoFar));
    assert!(!bases.contains(&BasePiece::UpperArmBack));
    assert!(bases.contains(&BasePiece::LowerArmBack));
    assert!(bases.contains(&BasePiece::HandBack));
    assert_eq!(attachment_ids(&back, &out.bound), vec!["shell".to_string()]);
}

#[test]
fn local_and_remote_share_back_plan() {
    let registry = pack();
    let equipment = present_slots(&[
        (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
        (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
        (EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
    ]);
    let mut set = CharacterPresentationSet::new();
    let local_key = PresentationEntityKey::new(3, 1);
    let remote_key = PresentationEntityKey::new(9, 2);
    set.sync(
        [
            (local_key, idle(equipment)),
            (
                remote_key,
                from_remote(
                    RemoteMotion {
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
    assert_eq!(local.state().view, PresentationView::Side);
    assert_eq!(remote.state().view, local.state().view);
    let lp = plan_character_draw(local.hidden_base(), local.bound(), PresentationView::Back);
    let rp = plan_character_draw(remote.hidden_base(), remote.bound(), PresentationView::Back);
    assert_eq!(base_sequence(&lp), base_sequence(&rp));
    assert_eq!(
        attachment_ids(&lp, local.bound()),
        attachment_ids(&rp, remote.bound())
    );
    let local_quads =
        presentation_debug_quads(set.bone_map(), local, 1.15, true, PresentationView::Back);
    let remote_quads =
        presentation_debug_quads(set.bone_map(), remote, 1.15, true, PresentationView::Back);
    assert_eq!(local_quads.len(), lp.len());
    assert_eq!(remote_quads.len(), rp.len());
    assert_eq!(local_quads.len(), remote_quads.len());
    let side_quads =
        presentation_debug_quads(set.bone_map(), local, 1.15, true, PresentationView::Side);
    assert!(side_quads.len() > local_quads.len());
}
