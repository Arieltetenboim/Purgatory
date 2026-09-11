//! Phase 8F-C activity → PresentationView. ClimbBack selects Back. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{ContentRegistry, LoadMode, default_content_root, load_registry};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;

use super::adapters::{
    apply_climb_back_overlay, apply_oneshot_overlay, from_local, from_local_with_oneshot,
    from_remote,
};
use super::bone_map::BoneTargetMap;
use super::debug_visual::presentation_debug_quads;
use super::draw_order::{BasePiece, PlannedKind, plan_character_draw};
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

fn kit() -> Vec<BoundAttachment> {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    resolve_equipment(
        present_slots(&[
            (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
            (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
            (EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
        ]),
        &registry,
        map,
    )
    .bound
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

#[test]
fn protocol_unchanged() {
    assert_eq!(PROTOCOL_VERSION, 26);
}

#[test]
fn activity_maps_to_presentation_view() {
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
    assert_eq!(
        view_for_activity(PresentationActivity::ClimbBack),
        PresentationView::Back
    );
}

#[test]
fn climb_back_sets_view_on_local_and_remote() {
    let local = apply_climb_back_overlay(idle(EquipmentView::Absent), true);
    let remote = apply_climb_back_overlay(
        from_remote(
            RemoteMotion {
                pose: [4.0, 0.0],
                velocity: [0.0, 0.0],
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
        ),
        true,
    );
    assert_eq!(local.activity, PresentationActivity::ClimbBack);
    assert_eq!(remote.activity, PresentationActivity::ClimbBack);
    assert_eq!(local.view, PresentationView::Back);
    assert_eq!(remote.view, local.view);
    assert_eq!(idle(EquipmentView::Absent).view, PresentationView::Side);
}

#[test]
fn climb_back_plan_matches_back_visibility_and_equipment() {
    let bound = kit();
    let state = apply_climb_back_overlay(idle(EquipmentView::empty_present()), true);
    let plan = plan_character_draw(0, &bound, state.view);
    let expected = plan_character_draw(0, &bound, PresentationView::Back);
    assert_eq!(plan, expected);
    let bases = base_sequence(&plan);
    assert!(!bases.contains(&BasePiece::UpperArmFront));
    assert!(!bases.contains(&BasePiece::FootFront));
    assert!(bases.contains(&BasePiece::UpperArmBack));
    assert!(bases.contains(&BasePiece::FootBack));
    let ids = attachment_ids(&plan, &bound);
    assert!(!ids.contains(&"blade".to_string()));
    assert!(!ids.contains(&"foot_front".to_string()));
    assert!(!ids.contains(&"hand_front".to_string()));
    assert!(ids.contains(&"foot_back".to_string()));
    assert!(
        !ids.contains(&"hand_back".to_string()),
        "Side-only glove must not Side-as-Back"
    );
}

#[test]
fn side_activities_keep_side_plan() {
    let bound = kit();
    let idle_state = idle(EquipmentView::Absent);
    assert_eq!(idle_state.activity, PresentationActivity::Idle);
    assert_eq!(idle_state.view, PresentationView::Side);
    let attack = from_local_with_oneshot(
        LocalMotion {
            pose: [0.0, 0.0],
            velocity: [6.0, 0.0],
            grounded: true,
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
        Some(PresentationActivity::Attack),
    );
    assert_eq!(attack.activity, PresentationActivity::Attack);
    assert_eq!(attack.view, PresentationView::Side);
    let side_plan = plan_character_draw(0, &bound, attack.view);
    assert_eq!(base_sequence(&side_plan), BasePiece::ALL.to_vec());
    assert!(attachment_ids(&side_plan, &bound).contains(&"blade".to_string()));
}

#[test]
fn local_and_remote_share_climb_back_draw() {
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
            (local_key, apply_climb_back_overlay(idle(equipment), true)),
            (
                remote_key,
                apply_climb_back_overlay(
                    from_remote(
                        RemoteMotion {
                            pose: [4.0, 0.0],
                            velocity: [0.0, 0.0],
                            equipment,
                        },
                        Facing::Right,
                    ),
                    true,
                ),
            ),
        ],
        &registry,
        0.0,
    );
    let local = set.get(local_key).unwrap();
    let remote = set.get(remote_key).unwrap();
    assert_eq!(local.state().activity, PresentationActivity::ClimbBack);
    assert_eq!(remote.state().activity, local.state().activity);
    assert_eq!(local.state().view, PresentationView::Back);
    assert_eq!(remote.state().view, local.state().view);
    let lp = plan_character_draw(local.hidden_base(), local.bound(), local.state().view);
    let rp = plan_character_draw(remote.hidden_base(), remote.bound(), remote.state().view);
    assert_eq!(base_sequence(&lp), base_sequence(&rp));
    assert_eq!(
        attachment_ids(&lp, local.bound()),
        attachment_ids(&rp, remote.bound())
    );
    let local_quads =
        presentation_debug_quads(set.bone_map(), local, 1.15, true, local.state().view);
    let remote_quads =
        presentation_debug_quads(set.bone_map(), remote, 1.15, true, remote.state().view);
    assert_eq!(local_quads.len(), lp.len());
    assert_eq!(remote_quads.len(), local_quads.len());
}

#[test]
fn force_back_draw_does_not_mutate_activity() {
    let registry = pack();
    let key = PresentationEntityKey::new(1, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, idle(EquipmentView::Absent))], &registry, 0.0);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.state().activity, PresentationActivity::Idle);
    assert_eq!(entry.state().view, PresentationView::Side);
    let forced =
        presentation_debug_quads(set.bone_map(), entry, 1.15, true, PresentationView::Back);
    assert_eq!(entry.state().activity, PresentationActivity::Idle);
    assert_eq!(entry.state().view, PresentationView::Side);
    let semantic = presentation_debug_quads(set.bone_map(), entry, 1.15, true, entry.state().view);
    assert!(forced.len() < semantic.len());
}

#[test]
fn oneshot_wins_over_climb_back_overlay() {
    let loc = apply_oneshot_overlay(
        PresentationActivity::ClimbBack,
        Some(PresentationActivity::Attack),
    );
    assert_eq!(loc, PresentationActivity::Attack);
    assert_eq!(view_for_activity(loc), PresentationView::Side);
    let ignored = apply_oneshot_overlay(
        PresentationActivity::Idle,
        Some(PresentationActivity::ClimbBack),
    );
    assert_eq!(ignored, PresentationActivity::Idle);
}
