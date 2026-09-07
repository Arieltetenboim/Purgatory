//! Phase 8F-D ClimbBack → climb_back.anim. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{ContentRegistry, LoadMode, default_content_root, load_registry};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;
use purgatory_skeleton::PELVIS;

use super::adapters::{apply_climb_back_overlay, from_local, from_remote};
use super::collection::clip_for_playback_activity;
use super::debug_visual::presentation_debug_quads;
use super::draw_order::{BasePiece, PlannedKind, plan_character_draw};
use super::state::{EquipmentView, Facing, PresentationActivity, PresentationView};
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

/// Sample time from the authored clip: first pelvis `tx` key that leaves bind.
fn climb_back_tx_sample_t() -> f32 {
    purgatory_animation::climb_back_clip()
        .tracks()
        .iter()
        .find(|track| track.bone == PELVIS)
        .and_then(|track| track.translation_x.as_ref())
        .and_then(|ch| {
            ch.keys()
                .iter()
                .find(|k| k.value.abs() > 0.1)
                .map(|k| k.time)
        })
        .expect("authored climb_back pelvis tx leaves bind")
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

fn climb(equipment: EquipmentView) -> super::state::CharacterPresentationState {
    apply_climb_back_overlay(idle(equipment), true)
}

#[test]
fn protocol_unchanged() {
    assert_eq!(PROTOCOL_VERSION, 20);
}

#[test]
fn climb_back_selects_climb_back_clip_not_idle() {
    let climb_clip = clip_for_playback_activity(PresentationActivity::ClimbBack);
    let idle_clip = clip_for_playback_activity(PresentationActivity::Idle);
    assert!(!std::ptr::eq(climb_clip, idle_clip));
    assert!(std::ptr::eq(
        climb_clip,
        purgatory_animation::climb_back_clip()
    ));
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Move) as *const _,
        purgatory_animation::a3_move_clip() as *const _
    );
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Jump) as *const _,
        purgatory_animation::a4_jump_clip() as *const _
    );
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Fall) as *const _,
        purgatory_animation::a4_fall_clip() as *const _
    );
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Attack) as *const _,
        purgatory_animation::a5_attack_clip() as *const _
    );
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Hurt) as *const _,
        purgatory_animation::a5_hurt_clip() as *const _
    );
}

#[test]
fn climb_back_sampled_pose_reaches_presentation_set() {
    let registry = pack();
    let key = PresentationEntityKey::new(1, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, climb(EquipmentView::Absent))],
        &registry,
        climb_back_tx_sample_t(),
    );
    let entry = set.get(key).unwrap();
    assert_eq!(entry.state().activity, PresentationActivity::ClimbBack);
    assert_eq!(entry.state().view, PresentationView::Back);
    assert_eq!(entry.playback_activity(), PresentationActivity::ClimbBack);
    assert!((entry.selected_sample_t() - climb_back_tx_sample_t()).abs() < 1e-4);
    let pelvis = entry.prepared().local.get(PELVIS).unwrap();
    assert!(
        pelvis.translation[0].abs() > 0.1,
        "climb_back pelvis tx should leave bind at an authored tx key"
    );
}

#[test]
fn local_and_remote_select_the_same_climb_back_clip() {
    let registry = pack();
    let equipment = present_slots(&[(EquipmentSlot::Boots, cid("equipment.debug.iron_boots"))]);
    let mut set = CharacterPresentationSet::new();
    let local_key = PresentationEntityKey::new(3, 1);
    let remote_key = PresentationEntityKey::new(9, 2);
    set.sync(
        [
            (local_key, climb(equipment)),
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
        0.3,
    );
    let local = set.get(local_key).unwrap();
    let remote = set.get(remote_key).unwrap();
    assert_eq!(local.playback_activity(), PresentationActivity::ClimbBack);
    assert_eq!(remote.playback_activity(), local.playback_activity());
    assert_eq!(
        clip_for_playback_activity(local.playback_activity()) as *const _,
        clip_for_playback_activity(remote.playback_activity()) as *const _
    );
    assert!((local.selected_sample_t() - remote.selected_sample_t()).abs() < 1e-4);
    let lp = local.prepared().local.get(PELVIS).unwrap().translation[0];
    let rp = remote.prepared().local.get(PELVIS).unwrap().translation[0];
    assert!((lp - rp).abs() < 1e-4);
}

#[test]
fn climb_back_keeps_back_visibility_and_equipment() {
    let registry = pack();
    let equipment = present_slots(&[
        (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
        (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
        (EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
    ]);
    let key = PresentationEntityKey::new(4, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, climb(equipment))], &registry, 0.3);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.state().view, PresentationView::Back);
    let plan = plan_character_draw(entry.hidden_base(), entry.bound(), entry.state().view);
    let bases: Vec<_> = plan
        .iter()
        .filter_map(|k| match k {
            PlannedKind::Base(p) => Some(*p),
            PlannedKind::Attachment(_) => None,
        })
        .collect();
    assert!(!bases.contains(&BasePiece::HandFront));
    assert!(bases.contains(&BasePiece::HandBack));
    let ids: Vec<_> = plan
        .iter()
        .filter_map(|k| match k {
            PlannedKind::Attachment(i) => entry.bound().get(*i).map(|b| b.attachment_id.as_str()),
            PlannedKind::Base(_) => None,
        })
        .collect();
    assert!(!ids.contains(&"blade"));
    assert!(ids.contains(&"foot_back"));
    assert!(
        !ids.contains(&"hand_back"),
        "Side-only glove must not Side-as-Back"
    );
    assert!(!entry.bound().is_empty());
    let quads = presentation_debug_quads(set.bone_map(), entry, 1.15, true, entry.state().view);
    assert_eq!(quads.len(), plan.len());
    let idle_key = PresentationEntityKey::new(5, 1);
    set.sync(
        [(key, climb(equipment)), (idle_key, idle(equipment))],
        &registry,
        0.3,
    );
    let climb_entry = set.get(key).unwrap();
    let idle_entry = set.get(idle_key).unwrap();
    let climb_tx = climb_entry
        .prepared()
        .local
        .get(PELVIS)
        .unwrap()
        .translation[0];
    let idle_tx = idle_entry.prepared().local.get(PELVIS).unwrap().translation[0];
    assert!((climb_tx - idle_tx).abs() > 0.1);
}
