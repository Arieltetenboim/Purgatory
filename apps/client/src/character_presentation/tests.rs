//! Headless Phase 8D proofs. No wgpu / window.

use purgatory_animation::{DEAD_CLIP_DURATION, a5_attack_clip, dead_clip};
use purgatory_common::ContentId;
use purgatory_content::{BoneTarget, ContentRegistry};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;
use purgatory_skeleton::{HEAD, ROOT, TORSO, humanoid_v0_bone_by_label};

use super::adapters::{
    from_local, from_local_with_presentation, from_remote, from_remote_with_presentation,
};
use super::bone_map::BoneTargetMap;
use super::collection::clip_for_playback_activity;
use super::skeleton_input::{prepared_from_state, skeleton_input_from_state, skeleton_root};
use super::state::{
    CharacterPresentationState, EquipmentView, Facing, PresentationActivity, PresentationView,
};
use super::{CharacterPresentationSet, LocalMotion, PresentationEntityKey, RemoteMotion};

fn pose(x: f32, y: f32) -> [f32; 2] {
    [x, y]
}

fn local_idle(equipment: EquipmentView) -> LocalMotion {
    LocalMotion {
        pose: pose(1.0, 2.0),
        velocity: [0.0, 0.0],
        grounded: true,
        equipment,
    }
}

fn remote_idle(equipment: EquipmentView) -> RemoteMotion {
    RemoteMotion {
        pose: pose(4.0, 2.0),
        velocity: [0.0, 0.0],
        equipment,
    }
}

#[test]
fn protocol_version_is_current() {
    assert_eq!(PROTOCOL_VERSION, 26);
}

#[test]
fn local_character_builds_presentation_state() {
    let state = from_local(local_idle(EquipmentView::Absent), Facing::Right);
    assert_eq!(state.pose, pose(1.0, 2.0));
    assert_eq!(state.facing, Facing::Right);
    assert_eq!(state.activity, PresentationActivity::Idle);
    assert_eq!(state.view, PresentationView::Side);
    assert!(state.equipment.is_absent());
    assert!(state.is_placeholder_ready());
    assert_ne!(state.view, PresentationView::Back);
}

#[test]
fn remote_character_same_state_shape() {
    let local = from_local(local_idle(EquipmentView::empty_present()), Facing::Right);
    let remote = from_remote(remote_idle(EquipmentView::empty_present()), Facing::Right);
    assert_eq!(local.facing, remote.facing);
    assert_eq!(local.activity, remote.activity);
    assert_eq!(local.view, remote.view);
    assert_eq!(local.equipment, remote.equipment);
}

#[test]
fn local_and_remote_use_common_builder() {
    let local = from_local(
        LocalMotion {
            pose: pose(0.0, 0.0),
            velocity: [6.0, 0.0],
            grounded: true,
            equipment: EquipmentView::Absent,
        },
        Facing::Left,
    );
    let remote = from_remote(
        RemoteMotion {
            pose: pose(8.0, 0.0),
            velocity: [6.0, 0.0],
            equipment: EquipmentView::Absent,
        },
        Facing::Left,
    );
    let a = skeleton_input_from_state(local);
    let b = skeleton_input_from_state(remote);
    assert_eq!(a.facing, b.facing);
    assert_eq!(a.activity, b.activity);
    assert_eq!(a.view, b.view);
    assert_eq!(a.facing, Facing::Right);
    assert_eq!(a.activity, PresentationActivity::Move);
}

#[test]
fn equipment_absent_and_empty_present_are_valid() {
    let absent = from_local(local_idle(EquipmentView::Absent), Facing::Right);
    let empty = from_local(local_idle(EquipmentView::empty_present()), Facing::Right);
    assert!(absent.equipment.is_absent());
    assert!(empty.equipment.is_empty_present());
    assert!(prepared_from_state(absent).is_ok());
    assert!(prepared_from_state(empty).is_ok());
}

#[test]
fn partial_equipment_is_carried_without_resolution() {
    let mut slots = [None; EquipmentSlot::COUNT];
    slots[EquipmentSlot::Weapon.index()] = Some(ContentId::from_token(7));
    slots[EquipmentSlot::Bodywear.index()] = Some(ContentId::from_token(3));
    let state = from_remote(remote_idle(EquipmentView::present(slots)), Facing::Right);
    assert_eq!(
        state.equipment.slot(EquipmentSlot::Weapon),
        Some(ContentId::from_token(7))
    );
    assert_eq!(
        state.equipment.slot(EquipmentSlot::Bodywear),
        Some(ContentId::from_token(3))
    );
    assert!(state.equipment.slot(EquipmentSlot::Headwear).is_none());
}

#[test]
fn facing_propagates_and_holds_when_idle() {
    let moving_left = from_local(
        LocalMotion {
            pose: pose(0.0, 0.0),
            velocity: [-6.0, 0.0],
            grounded: true,
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
    );
    assert_eq!(moving_left.facing, Facing::Left);
    let idle_hold = from_local(local_idle(EquipmentView::Absent), Facing::Left);
    assert_eq!(idle_hold.facing, Facing::Left);
}

#[test]
fn activity_propagates_from_local_grounded_and_remote_velocity() {
    let jump = from_local(
        LocalMotion {
            pose: pose(0.0, 1.0),
            velocity: [0.0, 8.0],
            grounded: false,
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
    );
    assert_eq!(jump.activity, PresentationActivity::Jump);
    let fall = from_remote(
        RemoteMotion {
            pose: pose(0.0, 3.0),
            velocity: [0.0, -4.0],
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
    );
    assert_eq!(fall.activity, PresentationActivity::Fall);
}

#[test]
fn skeleton_input_evaluates_without_equipment_or_art() {
    let state = from_local(local_idle(EquipmentView::Absent), Facing::Right);
    let prepared = prepared_from_state(state).expect("bind evaluate");
    assert_eq!(prepared.input.activity, PresentationActivity::Idle);
    assert_eq!(prepared.local().bone_count(), 16);
    assert_eq!(prepared.input.root_position, skeleton_root(state.pose));
    let root = prepared.world().get(ROOT).expect("root");
    assert!((root.translation[0] - 1.0).abs() < 1e-5);
    assert!(prepared.world().get(HEAD).is_some());
    assert!(prepared.world().get(TORSO).is_some());
}

#[test]
fn bone_target_maps_to_humanoid_indices() {
    let map = BoneTargetMap::bind_humanoid_v0().expect("humanoid v0 has all equipment bones");
    assert_eq!(map.bone(BoneTarget::Head), HEAD);
    assert_eq!(map.bone(BoneTarget::Torso), TORSO);
    assert_eq!(
        map.bone(BoneTarget::HandFront),
        humanoid_v0_bone_by_label("hand_front").unwrap()
    );
    assert_eq!(
        map.bone(BoneTarget::FootBack),
        humanoid_v0_bone_by_label("foot_back").unwrap()
    );
    assert!(humanoid_v0_bone_by_label("root").is_some());
    assert!(humanoid_v0_bone_by_label("pelvis").is_some());
}

#[test]
fn missing_canonical_bone_fails_clearly() {
    fn no_head(label: &str) -> Option<purgatory_skeleton::BoneIndex> {
        if label == "head" {
            None
        } else {
            humanoid_v0_bone_by_label(label)
        }
    }
    let err = BoneTargetMap::bind_with_lookup(no_head).expect_err("head required");
    assert_eq!(err.label, "head");
    assert!(!err.to_string().is_empty());
}

#[test]
fn two_characters_share_path_and_remote_leave_drops_entry() {
    let a_key = PresentationEntityKey::new(1, 1);
    let b_key = PresentationEntityKey::new(2, 1);
    let mut slots_a = [None; EquipmentSlot::COUNT];
    slots_a[EquipmentSlot::Weapon.index()] = Some(ContentId::from_token(11));
    let mut slots_b = [None; EquipmentSlot::COUNT];
    slots_b[EquipmentSlot::Bodywear.index()] = Some(ContentId::from_token(22));
    let a = from_local(
        LocalMotion {
            pose: pose(0.0, 0.0),
            velocity: [6.0, 0.0],
            grounded: true,
            equipment: EquipmentView::present(slots_a),
        },
        Facing::Right,
    );
    let b = from_remote(
        RemoteMotion {
            pose: pose(3.0, 0.0),
            velocity: [0.0, 0.0],
            equipment: EquipmentView::present(slots_b),
        },
        Facing::Right,
    );
    let mut set = CharacterPresentationSet::new();
    assert_eq!(set.bone_map().bone(BoneTarget::Head), HEAD);
    set.sync([(a_key, a), (b_key, b)], &ContentRegistry::new(), 0.0);
    assert_eq!(set.len(), 2);
    assert_eq!(set.iter().count(), 2);
    let prepared_a = set.get(a_key).unwrap().prepared();
    assert!(prepared_a.world.get(ROOT).is_some());
    assert_eq!(prepared_a.input.activity, PresentationActivity::Move);
    assert_eq!(prepared_a.local.bone_count(), 16);
    let a_in = set.get(a_key).unwrap().skeleton_input();
    let b_in = set.get(b_key).unwrap().skeleton_input();
    assert_eq!(a_in.view, b_in.view);
    assert_eq!(a_in.activity, PresentationActivity::Move);
    assert_eq!(b_in.activity, PresentationActivity::Idle);
    assert_eq!(
        set.get(a_key)
            .unwrap()
            .state()
            .equipment
            .slot(EquipmentSlot::Weapon),
        Some(ContentId::from_token(11))
    );
    assert_eq!(
        set.get(b_key)
            .unwrap()
            .state()
            .equipment
            .slot(EquipmentSlot::Bodywear),
        Some(ContentId::from_token(22))
    );
    set.sync([(a_key, a)], &ContentRegistry::new(), 0.0);
    assert_eq!(set.len(), 1);
    assert!(set.get(b_key).is_none());
    assert!(set.get(a_key).is_some());
}

#[test]
fn empty_collection_is_valid_placeholder_path() {
    let mut set = CharacterPresentationSet::new();
    set.sync([], &ContentRegistry::new(), 0.0);
    assert!(set.is_empty());
}

#[test]
fn a1_head_sample_rotates_local_world_head() {
    let key = PresentationEntityKey::new(1, 1);
    let state = from_local(local_idle(EquipmentView::Absent), Facing::Right);
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, state)], &ContentRegistry::new(), 0.0);
    let bind_rot = set
        .get(key)
        .unwrap()
        .prepared()
        .world
        .get(HEAD)
        .unwrap()
        .rotation;
    set.apply_a1_head_sample(key, 0.5);
    let sampled = set
        .get(key)
        .unwrap()
        .prepared()
        .world
        .get(HEAD)
        .unwrap()
        .rotation;
    assert!(
        (sampled - bind_rot).abs() > 0.2,
        "A1 sample must change world head rotation (bind={bind_rot}, sampled={sampled})"
    );
    let root_before = set
        .get(key)
        .unwrap()
        .prepared()
        .world
        .get(ROOT)
        .unwrap()
        .translation;
    set.apply_a1_head_sample(key, 1.0);
    let root_after = set
        .get(key)
        .unwrap()
        .prepared()
        .world
        .get(ROOT)
        .unwrap()
        .translation;
    assert_eq!(root_before, root_after);
}

#[test]
fn presentation_state_has_no_gpu_layout() {
    let size = std::mem::size_of::<CharacterPresentationState>();
    assert!(size < 128, "presentation state must stay small, got {size}");
}

fn idle_state() -> CharacterPresentationState {
    from_local(local_idle(EquipmentView::Absent), Facing::Right)
}

fn move_state(pose: [f32; 2]) -> CharacterPresentationState {
    from_local(
        LocalMotion {
            pose,
            velocity: [6.0, 0.0],
            grounded: true,
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
    )
}

#[test]
fn a3_idle_selects_idle_clip() {
    let key = PresentationEntityKey::new(1, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, idle_state())], &ContentRegistry::new(), 0.1);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.playback_activity(), PresentationActivity::Idle);
    assert_eq!(
        clip_for_playback_activity(entry.playback_activity()) as *const _,
        purgatory_animation::a3_idle_clip() as *const _
    );
}

#[test]
fn dead_selects_dead_clip_instead_of_hurt() {
    let key = PresentationEntityKey::new(6, 1);
    let mut set = CharacterPresentationSet::new();
    let dead = from_local_with_presentation(
        local_idle(EquipmentView::Absent),
        Facing::Right,
        Some(PresentationActivity::Hurt),
        true,
    );

    set.sync([(key, dead)], &ContentRegistry::new(), 0.1);
    let first = set.get(key).expect("dead entry");
    assert_eq!(first.state().activity, PresentationActivity::Dead);
    assert_eq!(first.playback_activity(), PresentationActivity::Dead);
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Dead) as *const _,
        dead_clip() as *const _
    );
    assert_eq!(dead_clip().duration(), DEAD_CLIP_DURATION);
    let first_t = first.selected_sample_t();

    set.sync([(key, dead)], &ContentRegistry::new(), 0.1);
    let second = set.get(key).expect("dead entry");
    assert_eq!(second.playback_activity(), PresentationActivity::Dead);
    assert!(second.selected_sample_t() > first_t);

    set.sync([(key, dead)], &ContentRegistry::new(), 1.0);
    let final_entry = set.get(key).expect("dead entry");
    assert_eq!(final_entry.selected_sample_t(), DEAD_CLIP_DURATION);
}

#[test]
fn remote_dead_health_resolves_dead_activity_and_clip() {
    let key = PresentationEntityKey::new(7, 1);
    let mut set = CharacterPresentationSet::new();
    let dead = from_remote_with_presentation(
        remote_idle(EquipmentView::Absent),
        Facing::Right,
        Some(PresentationActivity::Hurt),
        true,
    );

    set.sync([(key, dead)], &ContentRegistry::new(), 0.1);
    let entry = set.get(key).expect("remote dead entry");
    assert_eq!(entry.state().activity, PresentationActivity::Dead);
    assert_eq!(entry.playback_activity(), PresentationActivity::Dead);
    assert_eq!(
        clip_for_playback_activity(entry.playback_activity()) as *const _,
        dead_clip() as *const _
    );

    let alive = from_remote_with_presentation(
        remote_idle(EquipmentView::Absent),
        Facing::Right,
        None,
        false,
    );
    set.sync([(key, alive)], &ContentRegistry::new(), 0.1);
    let restored = set.get(key).expect("restored remote entry");
    assert_ne!(restored.state().activity, PresentationActivity::Dead);
    assert_eq!(restored.state().activity, PresentationActivity::Idle);
}

#[test]
fn a3_move_selects_move_clip() {
    let key = PresentationEntityKey::new(2, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.1,
    );
    let entry = set.get(key).unwrap();
    assert_eq!(entry.playback_activity(), PresentationActivity::Move);
    assert_eq!(
        clip_for_playback_activity(entry.playback_activity()) as *const _,
        purgatory_animation::a3_move_clip() as *const _
    );
}

#[test]
fn a3_idle_to_move_resets_once() {
    let key = PresentationEntityKey::new(3, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, idle_state())], &ContentRegistry::new(), 0.25);
    let idle_t = set.get(key).unwrap().selected_sample_t();
    assert!(idle_t > 0.0);
    set.sync(
        [(key, move_state(pose(1.0, 0.0)))],
        &ContentRegistry::new(),
        0.05,
    );
    let move_t = set.get(key).unwrap().selected_sample_t();
    assert!(
        (move_t - 0.05).abs() < 1e-4,
        "Idle→Move must reset then advance once, got {move_t}"
    );
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Move
    );
}

#[test]
fn a3_move_to_move_does_not_reset() {
    let key = PresentationEntityKey::new(4, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.2,
    );
    let t1 = set.get(key).unwrap().selected_sample_t();
    set.sync(
        [(key, move_state(pose(1.0, 0.0)))],
        &ContentRegistry::new(),
        0.2,
    );
    let t2 = set.get(key).unwrap().selected_sample_t();
    assert!(
        (t2 - (t1 + 0.2)).abs() < 1e-4,
        "repeated Move must continue playback (t1={t1}, t2={t2})"
    );
}

#[test]
fn a3_move_to_idle_resets_once() {
    let key = PresentationEntityKey::new(5, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.3,
    );
    set.sync([(key, idle_state())], &ContentRegistry::new(), 0.05);
    let idle_t = set.get(key).unwrap().selected_sample_t();
    assert!(
        (idle_t - 0.05).abs() < 1e-4,
        "Move→Idle must reset then advance once, got {idle_t}"
    );
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Idle
    );
}

#[test]
fn a3_two_characters_independent_playback() {
    let a = PresentationEntityKey::new(10, 1);
    let b = PresentationEntityKey::new(11, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync([(a, idle_state())], &ContentRegistry::new(), 0.4);
    set.sync(
        [(a, idle_state()), (b, idle_state())],
        &ContentRegistry::new(),
        0.1,
    );
    let ta = set.get(a).unwrap().selected_sample_t();
    let tb = set.get(b).unwrap().selected_sample_t();
    assert!(
        (ta - tb).abs() > 0.2,
        "independent players must diverge (ta={ta}, tb={tb})"
    );
}

#[test]
fn a3_repeated_remote_style_move_preserves_progress() {
    let key = PresentationEntityKey::new(20, 1);
    let mut set = CharacterPresentationSet::new();
    let remote_move = |x: f32| {
        from_remote(
            RemoteMotion {
                pose: pose(x, 0.0),
                velocity: [6.0, 0.0],
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
        )
    };
    set.sync([(key, remote_move(0.0))], &ContentRegistry::new(), 0.15);
    let t1 = set.get(key).unwrap().selected_sample_t();
    set.sync([(key, remote_move(1.0))], &ContentRegistry::new(), 0.15);
    set.sync([(key, remote_move(2.0))], &ContentRegistry::new(), 0.15);
    let t3 = set.get(key).unwrap().selected_sample_t();
    assert!(
        (t3 - (t1 + 0.30)).abs() < 1e-4,
        "remote Move snapshots must not restart (t1={t1}, t3={t3})"
    );
}

fn jump_state(p: [f32; 2]) -> CharacterPresentationState {
    from_local(
        LocalMotion {
            pose: p,
            velocity: [0.0, 8.0],
            grounded: false,
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
    )
}

fn fall_state(p: [f32; 2]) -> CharacterPresentationState {
    from_local(
        LocalMotion {
            pose: p,
            velocity: [0.0, -8.0],
            grounded: false,
            equipment: EquipmentView::Absent,
        },
        Facing::Right,
    )
}

#[test]
fn a4_activity_maps_idle_move_jump_fall() {
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Jump) as *const _,
        purgatory_animation::a4_jump_clip() as *const _
    );
    assert_eq!(
        clip_for_playback_activity(PresentationActivity::Fall) as *const _,
        purgatory_animation::a4_fall_clip() as *const _
    );
}

#[test]
fn a4_move_to_jump_starts_one_transition() {
    let key = PresentationEntityKey::new(30, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.2,
    );
    assert!(!set.get(key).unwrap().transitioning());
    set.sync(
        [(key, jump_state(pose(0.0, 1.0)))],
        &ContentRegistry::new(),
        0.05,
    );
    let entry = set.get(key).unwrap();
    assert!(entry.transitioning());
    assert!((entry.transition_elapsed() - 0.05).abs() < 1e-4);
    assert_eq!(entry.playback_activity(), PresentationActivity::Jump);
    assert!((entry.selected_sample_t() - 0.05).abs() < 1e-4);
}

#[test]
fn a4_jump_to_fall_transitions_without_same_state_restart() {
    let key = PresentationEntityKey::new(31, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, jump_state(pose(0.0, 1.0)))],
        &ContentRegistry::new(),
        0.2,
    );
    set.sync(
        [(key, fall_state(pose(0.0, 2.0)))],
        &ContentRegistry::new(),
        0.04,
    );
    assert!(set.get(key).unwrap().transitioning());
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Fall
    );
    set.sync(
        [(key, fall_state(pose(0.0, 3.0)))],
        &ContentRegistry::new(),
        0.04,
    );
    let entry = set.get(key).unwrap();
    assert!(entry.transitioning());
    assert!(
        (entry.selected_sample_t() - 0.08).abs() < 1e-4,
        "Fall→Fall must not reset player, got {}",
        entry.selected_sample_t()
    );
}

#[test]
fn a4_fall_to_idle_and_move_transition() {
    let key = PresentationEntityKey::new(32, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, fall_state(pose(0.0, 2.0)))],
        &ContentRegistry::new(),
        0.1,
    );
    set.sync([(key, idle_state())], &ContentRegistry::new(), 0.05);
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Idle
    );
    assert!(set.get(key).unwrap().transitioning());
    set.sync(
        [(key, move_state(pose(1.0, 0.0)))],
        &ContentRegistry::new(),
        0.05,
    );
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Move
    );
    assert!(set.get(key).unwrap().transitioning());
    assert!((set.get(key).unwrap().transition_elapsed() - 0.05).abs() < 1e-4);
}

#[test]
fn a4_same_activity_does_not_reblend() {
    let key = PresentationEntityKey::new(33, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.1,
    );
    set.sync(
        [(key, move_state(pose(1.0, 0.0)))],
        &ContentRegistry::new(),
        0.1,
    );
    assert!(!set.get(key).unwrap().transitioning());
    set.sync(
        [(key, jump_state(pose(0.0, 1.0)))],
        &ContentRegistry::new(),
        0.04,
    );
    assert!(set.get(key).unwrap().transitioning());
    set.sync(
        [(key, jump_state(pose(0.0, 1.5)))],
        &ContentRegistry::new(),
        0.04,
    );
    let entry = set.get(key).unwrap();
    assert!(entry.transitioning());
    assert!(
        (entry.transition_elapsed() - 0.08).abs() < 1e-4,
        "Jump→Jump must continue same transition, got {}",
        entry.transition_elapsed()
    );
}

#[test]
fn a4_interrupt_restarts_transition_from_current_pose() {
    use purgatory_skeleton::UPPER_ARM_FRONT;
    let key = PresentationEntityKey::new(34, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.25,
    );
    set.sync(
        [(key, jump_state(pose(0.0, 1.0)))],
        &ContentRegistry::new(),
        0.05,
    );
    let mid_arm = set
        .get(key)
        .unwrap()
        .prepared()
        .local
        .get(UPPER_ARM_FRONT)
        .unwrap()
        .rotation;
    set.sync(
        [(key, fall_state(pose(0.0, 2.0)))],
        &ContentRegistry::new(),
        0.0,
    );
    // After interrupt with dt=0: alpha=0 → presented equals captured mid-blend pose.
    let after = set
        .get(key)
        .unwrap()
        .prepared()
        .local
        .get(UPPER_ARM_FRONT)
        .unwrap()
        .rotation;
    assert!(
        (after - mid_arm).abs() < 1e-4,
        "interrupt must start from current blended pose (mid={mid_arm}, after={after})"
    );
    assert!(set.get(key).unwrap().transitioning());
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Fall
    );
}

#[test]
fn a4_transition_completes_to_target_and_evaluate_ok() {
    use purgatory_skeleton::UPPER_ARM_FRONT;
    let key = PresentationEntityKey::new(35, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [(key, move_state(pose(0.0, 0.0)))],
        &ContentRegistry::new(),
        0.2,
    );
    set.sync(
        [(key, jump_state(pose(0.0, 1.0)))],
        &ContentRegistry::new(),
        0.12,
    );
    let entry = set.get(key).unwrap();
    assert!(!entry.transitioning());
    let arm = entry
        .prepared()
        .local
        .get(UPPER_ARM_FRONT)
        .unwrap()
        .rotation;
    assert!(
        arm > 0.5,
        "completed Jump transition should reach raised-arm target, got {arm}"
    );
    let root = entry.prepared().local.get(ROOT).unwrap().translation;
    let expected = skeleton_root(pose(0.0, 1.0));
    assert_eq!(root, expected);
    assert!(
        entry
            .prepared()
            .world
            .get(HEAD)
            .unwrap()
            .rotation
            .is_finite()
    );
}

#[test]
fn a4_two_characters_independent_transition_state() {
    let a = PresentationEntityKey::new(40, 1);
    let b = PresentationEntityKey::new(41, 1);
    let mut set = CharacterPresentationSet::new();
    set.sync(
        [
            (a, move_state(pose(0.0, 0.0))),
            (b, move_state(pose(5.0, 0.0))),
        ],
        &ContentRegistry::new(),
        0.2,
    );
    set.sync(
        [
            (a, jump_state(pose(0.0, 1.0))),
            (b, move_state(pose(6.0, 0.0))),
        ],
        &ContentRegistry::new(),
        0.05,
    );
    assert!(set.get(a).unwrap().transitioning());
    assert!(!set.get(b).unwrap().transitioning());
    assert_eq!(
        set.get(a).unwrap().playback_activity(),
        PresentationActivity::Jump
    );
    assert_eq!(
        set.get(b).unwrap().playback_activity(),
        PresentationActivity::Move
    );
}

fn attack_state(pose: [f32; 2]) -> CharacterPresentationState {
    CharacterPresentationState {
        pose,
        facing: Facing::Right,
        activity: PresentationActivity::Attack,
        view: PresentationView::Side,
        equipment: EquipmentView::Absent,
    }
}

fn hurt_state(pose: [f32; 2]) -> CharacterPresentationState {
    CharacterPresentationState {
        pose,
        facing: Facing::Right,
        activity: PresentationActivity::Hurt,
        view: PresentationView::Side,
        equipment: EquipmentView::Absent,
    }
}

#[test]
fn a5_attack_selects_oneshot_and_resets_once() {
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(1, 1);
    set.sync([(key, attack_state([0.0, 1.0]))], &registry, 0.05);
    let t0 = set.get(key).unwrap().selected_sample_t();
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Attack
    );
    assert!(std::ptr::eq(
        clip_for_playback_activity(PresentationActivity::Attack),
        a5_attack_clip()
    ));
    set.sync([(key, attack_state([0.0, 1.0]))], &registry, 0.05);
    let t1 = set.get(key).unwrap().selected_sample_t();
    assert!(t1 > t0, "same Attack must continue, not reset");
    assert!(!set.get(key).unwrap().transitioning() || t1 > 0.0);
}

#[test]
fn a5_hurt_interrupts_attack_and_resets() {
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(2, 1);
    set.sync([(key, attack_state([1.0, 1.0]))], &registry, 0.10);
    set.sync([(key, hurt_state([1.0, 1.0]))], &registry, 0.05);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.playback_activity(), PresentationActivity::Hurt);
    assert!(entry.transitioning());
    assert!(
        (entry.selected_sample_t() - 0.05).abs() < 1e-4,
        "Hurt entry must reset player once"
    );
}

#[test]
fn a5_attack_does_not_override_active_hurt_in_presentation() {
    // Presentation shows whatever semantic state is supplied. Policy lives on
    // the server; if Hurt remains the resolved activity, Attack is not shown.
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(3, 1);
    set.sync([(key, hurt_state([0.0, 1.0]))], &registry, 0.05);
    set.sync([(key, hurt_state([0.0, 1.0]))], &registry, 0.05);
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Hurt
    );
}

#[test]
fn a5_clip_end_holds_final_pose_while_semantic_active() {
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(4, 1);
    set.sync([(key, attack_state([0.0, 1.0]))], &registry, 0.01);
    // Advance past Attack clip duration while activity stays Attack.
    for _ in 0..50 {
        set.sync([(key, attack_state([0.0, 1.0]))], &registry, 0.05);
    }
    let entry = set.get(key).unwrap();
    let clip = a5_attack_clip();
    assert_eq!(entry.playback_activity(), PresentationActivity::Attack);
    assert!(
        (entry.selected_sample_t() - clip.duration()).abs() < 1e-3,
        "Once policy must hold at duration, got {}",
        entry.selected_sample_t()
    );
    let arm = entry
        .prepared()
        .local
        .get(purgatory_skeleton::UPPER_ARM_FRONT)
        .map(|t| t.rotation)
        .unwrap_or(0.0);
    assert!(
        (arm - 0.55).abs() < 0.05,
        "held Attack end pose should keep front arm swung forward (+X), got {arm}"
    );
}

#[test]
fn a5_semantic_end_returns_to_locomotion_with_blend() {
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(5, 1);
    set.sync([(key, attack_state([0.0, 1.0]))], &registry, 0.10);
    set.sync([(key, idle_state())], &registry, 0.05);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.playback_activity(), PresentationActivity::Idle);
    assert!(entry.transitioning());
    assert!((entry.transition_elapsed() - 0.05).abs() < 1e-4);

    set.sync([(key, attack_state([2.0, 1.0]))], &registry, 0.10);
    set.sync([(key, move_state([2.0, 1.0]))], &registry, 0.05);
    assert_eq!(
        set.get(key).unwrap().playback_activity(),
        PresentationActivity::Move
    );
    assert!(set.get(key).unwrap().transitioning());
}

#[test]
fn a5_two_characters_independent_oneshot_state() {
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let a = PresentationEntityKey::new(10, 1);
    let b = PresentationEntityKey::new(11, 1);
    set.sync(
        [(a, attack_state([0.0, 1.0])), (b, idle_state())],
        &registry,
        0.05,
    );
    set.sync(
        [(a, attack_state([0.0, 1.0])), (b, hurt_state([3.0, 1.0]))],
        &registry,
        0.05,
    );
    assert_eq!(
        set.get(a).unwrap().playback_activity(),
        PresentationActivity::Attack
    );
    assert_eq!(
        set.get(b).unwrap().playback_activity(),
        PresentationActivity::Hurt
    );
    assert!(!set.get(a).unwrap().transitioning());
    assert!(set.get(b).unwrap().transitioning());
}

#[test]
fn a5_root_independent_and_evaluate_ok() {
    let registry = ContentRegistry::new();
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(12, 1);
    set.sync([(key, attack_state([7.5, 2.25]))], &registry, 0.05);
    set.sync([(key, hurt_state([7.5, 2.25]))], &registry, 0.05);
    let prepared = set.get(key).unwrap().prepared();
    let expected_root = skeleton_root([7.5, 2.25]);
    assert_eq!(prepared.input.root_position, expected_root);
    let root = prepared.local.get(purgatory_skeleton::ROOT).expect("root");
    assert_eq!(root.translation, expected_root);
    assert_eq!(root.rotation, 0.0);
}
