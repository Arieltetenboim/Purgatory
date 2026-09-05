//! S1 tests: definition validation, 2D composition, Humanoid v0 slots, pose isolation.

use super::*;

const EPS: f32 = 1e-5;

fn assert_tr_eq(actual: BoneTransform, expected: BoneTransform) {
    assert!(
        (actual.translation[0] - expected.translation[0]).abs() < EPS,
        "tx {} vs {}",
        actual.translation[0],
        expected.translation[0]
    );
    assert!(
        (actual.translation[1] - expected.translation[1]).abs() < EPS,
        "ty {} vs {}",
        actual.translation[1],
        expected.translation[1]
    );
    assert!(
        (actual.rotation - expected.rotation).abs() < EPS,
        "rot {} vs {}",
        actual.rotation,
        expected.rotation
    );
}

fn chain_def() -> SkeletonDef {
    let b0 = BoneIndex::from_u8(0);
    let b1 = BoneIndex::from_u8(1);
    let b2 = BoneIndex::from_u8(2);
    SkeletonDef::try_new(
        vec![None, Some(b0), Some(b1), Some(b2)],
        vec![
            BoneTransform::IDENTITY,
            BoneTransform::from_translation_rotation([1.0, 0.0], 0.0),
            BoneTransform::from_translation_rotation([1.0, 0.0], 0.0),
            BoneTransform::from_translation_rotation([1.0, 0.0], 0.0),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("valid 4-bone chain")
}

#[test]
fn empty_def_is_rejected() {
    let err = SkeletonDef::try_new(vec![], vec![], vec![], vec![]).unwrap_err();
    assert_eq!(err, SkeletonDefError::Empty);
}

#[test]
fn root_must_have_no_parent() {
    let err = SkeletonDef::try_new(
        vec![Some(BoneIndex::from_u8(0))],
        vec![BoneTransform::IDENTITY],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, SkeletonDefError::RootHasParent);
}

#[test]
fn extra_root_is_rejected() {
    let err = SkeletonDef::try_new(
        vec![None, None],
        vec![BoneTransform::IDENTITY, BoneTransform::IDENTITY],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, SkeletonDefError::ExtraRoot { bone: 1 });
}

#[test]
fn parent_must_be_strictly_less_than_child() {
    let err = SkeletonDef::try_new(
        vec![None, Some(BoneIndex::from_u8(1))],
        vec![BoneTransform::IDENTITY, BoneTransform::IDENTITY],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(
        err,
        SkeletonDefError::ParentNotStrictlyLess { bone: 1, parent: 1 }
    );
}

#[test]
fn parent_ahead_of_child_is_rejected() {
    let err = SkeletonDef::try_new(
        vec![
            None,
            Some(BoneIndex::from_u8(0)),
            Some(BoneIndex::from_u8(3)),
            Some(BoneIndex::from_u8(0)),
        ],
        vec![BoneTransform::IDENTITY; 4],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(
        err,
        SkeletonDefError::ParentNotStrictlyLess { bone: 2, parent: 3 }
    );
}

#[test]
fn non_finite_bind_is_rejected() {
    let err = SkeletonDef::try_new(
        vec![None],
        vec![BoneTransform::from_translation_rotation(
            [f32::NAN, 0.0],
            0.0,
        )],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(err, SkeletonDefError::NonFiniteBind { bone: 0 });
}

#[test]
fn non_finite_slot_rest_is_rejected() {
    let err = SkeletonDef::try_new(
        vec![None],
        vec![BoneTransform::IDENTITY],
        vec![SlotDef {
            bone: BoneIndex::from_u8(0),
            rest: BoneTransform::from_translation_rotation([0.0, f32::INFINITY], 0.0),
        }],
        vec![SlotIndex::from_u8(0)],
    )
    .unwrap_err();
    assert_eq!(err, SkeletonDefError::NonFiniteSlotRest { slot: 0 });
}

#[test]
fn bind_pose_evaluation_is_deterministic() {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world_a = WorldPose::new(def);
    let mut world_b = WorldPose::new(def);
    evaluate(def, &local, &mut world_a).unwrap();
    evaluate(def, &local, &mut world_b).unwrap();
    for i in 0..def.bone_count() {
        let idx = BoneIndex::from_u8(u8::try_from(i).unwrap());
        assert_tr_eq(world_a.get(idx).unwrap(), world_b.get(idx).unwrap());
    }
    assert_tr_eq(local.get(ROOT).unwrap(), BIND_ROOT);
    assert_tr_eq(local.get(PELVIS).unwrap(), BIND_PELVIS);
}

#[test]
fn copy_bind_rewrites_existing_local_buffer() {
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    local.get_mut(FOOT_FRONT).unwrap().translation = [9.0, 9.0];
    local.copy_bind(def).unwrap();
    assert_tr_eq(local.get(FOOT_FRONT).unwrap(), BIND_FOOT_FRONT);
}

#[test]
fn child_world_follows_parent_translation() {
    let def = chain_def();
    let local = LocalPose::from_bind(&def);
    let mut world = WorldPose::new(&def);
    evaluate(&def, &local, &mut world).unwrap();
    assert_tr_eq(
        world.get(BoneIndex::from_u8(1)).unwrap(),
        BoneTransform::from_translation_rotation([1.0, 0.0], 0.0),
    );
    assert_tr_eq(
        world.get(BoneIndex::from_u8(3)).unwrap(),
        BoneTransform::from_translation_rotation([3.0, 0.0], 0.0),
    );
}

#[test]
fn child_world_follows_parent_rotation() {
    let b0 = BoneIndex::from_u8(0);
    let b1 = BoneIndex::from_u8(1);
    let def = SkeletonDef::try_new(
        vec![None, Some(b0)],
        vec![
            BoneTransform::from_translation_rotation([0.0, 0.0], core::f32::consts::FRAC_PI_2),
            BoneTransform::from_translation_rotation([1.0, 0.0], 0.0),
        ],
        vec![],
        vec![],
    )
    .unwrap();
    let local = LocalPose::from_bind(&def);
    let mut world = WorldPose::new(&def);
    evaluate(&def, &local, &mut world).unwrap();
    let child = world.get(b1).unwrap();
    assert!((child.translation[0] - 0.0).abs() < EPS);
    assert!((child.translation[1] - 1.0).abs() < EPS);
    assert!((child.rotation - core::f32::consts::FRAC_PI_2).abs() < EPS);
}

#[test]
fn nested_transforms_compose_across_generations() {
    let def = chain_def();
    let mut local = LocalPose::from_bind(&def);
    local.get_mut(BoneIndex::from_u8(1)).unwrap().rotation = core::f32::consts::FRAC_PI_2;
    let mut world = WorldPose::new(&def);
    evaluate(&def, &local, &mut world).unwrap();
    let b2 = world.get(BoneIndex::from_u8(2)).unwrap();
    let b3 = world.get(BoneIndex::from_u8(3)).unwrap();
    assert!((b2.translation[0] - 1.0).abs() < EPS);
    assert!((b2.translation[1] - 1.0).abs() < EPS);
    assert!((b3.translation[0] - 1.0).abs() < EPS);
    assert!((b3.translation[1] - 2.0).abs() < EPS);
}

#[test]
fn foot_front_local_change_does_not_alter_shin_local() {
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let shin_before = local.get(LOWER_LEG_FRONT).unwrap();
    {
        let foot = local.get_mut(FOOT_FRONT).unwrap();
        foot.translation = [0.2, 0.15];
        foot.rotation = 0.4;
    }
    assert_tr_eq(local.get(LOWER_LEG_FRONT).unwrap(), shin_before);

    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let bind_local = LocalPose::from_bind(def);
    let mut bind_world = WorldPose::new(def);
    evaluate(def, &bind_local, &mut bind_world).unwrap();
    assert_tr_eq(
        world.get(LOWER_LEG_FRONT).unwrap(),
        bind_world.get(LOWER_LEG_FRONT).unwrap(),
    );
    assert_ne!(
        world.get(FOOT_FRONT).unwrap().translation,
        bind_world.get(FOOT_FRONT).unwrap().translation
    );
}

#[test]
fn shoe_front_slot_resolves_through_foot_front() {
    let def = humanoid_v0();
    assert_eq!(def.slot(SLOT_SHOE_FRONT).unwrap().bone, FOOT_FRONT);
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let expected = world.get(FOOT_FRONT).unwrap().compose(SLOT_REST_SHOE_FRONT);
    assert_tr_eq(slot_world(def, &world, SLOT_SHOE_FRONT).unwrap(), expected);
}

#[test]
fn weapon_slot_resolves_through_hand_front() {
    let def = humanoid_v0();
    assert_eq!(def.slot(SLOT_WEAPON).unwrap().bone, HAND_FRONT);
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let expected = world.get(HAND_FRONT).unwrap().compose(SLOT_REST_WEAPON);
    assert_tr_eq(slot_world(def, &world, SLOT_WEAPON).unwrap(), expected);
}

#[test]
fn pose_buffers_sharing_a_definition_do_not_affect_each_other() {
    let def = humanoid_v0();
    let mut local_a = LocalPose::from_bind(def);
    let local_b = LocalPose::from_bind(def);
    local_a.get_mut(FOOT_FRONT).unwrap().translation = [1.5, -0.3];
    let mut world_a = WorldPose::new(def);
    let mut world_b = WorldPose::new(def);
    evaluate(def, &local_a, &mut world_a).unwrap();
    evaluate(def, &local_b, &mut world_b).unwrap();
    assert_ne!(
        world_a.get(FOOT_FRONT).unwrap().translation,
        world_b.get(FOOT_FRONT).unwrap().translation
    );
    assert_tr_eq(local_b.get(FOOT_FRONT).unwrap(), BIND_FOOT_FRONT);
}

#[test]
fn evaluate_rejects_mismatched_bone_counts() {
    let def = humanoid_v0();
    let local = LocalPose::with_identity(3);
    let mut world = WorldPose::new(def);
    let err = evaluate(def, &local, &mut world).unwrap_err();
    assert_eq!(
        err,
        PoseError::BoneCountMismatch {
            definition: def.bone_count(),
            local: 3,
            world: def.bone_count(),
        }
    );
}

#[test]
fn evaluate_rewrites_caller_owned_world_buffer() {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    let count = world.bone_count();
    evaluate(def, &local, &mut world).unwrap();
    let first = world.get(HEAD).unwrap();
    evaluate(def, &local, &mut world).unwrap();
    assert_eq!(world.bone_count(), count);
    assert_tr_eq(world.get(HEAD).unwrap(), first);
}

#[test]
fn humanoid_v0_counts_and_draw_order_are_independent_of_hierarchy() {
    let def = humanoid_v0();
    assert_eq!(def.bone_count(), usize::from(BONE_COUNT));
    assert_eq!(def.slot_count(), usize::from(SLOT_COUNT));
    assert_eq!(def.parent(ROOT), None);
    assert_eq!(def.parent(PELVIS), Some(ROOT));
    assert_eq!(def.parent(FOOT_FRONT), Some(LOWER_LEG_FRONT));
    assert_eq!(def.parent(HAND_FRONT), Some(LOWER_ARM_FRONT));
    assert_eq!(def.draw_order()[0], SLOT_HAIR_BACK);
    assert_ne!(def.draw_order()[0], SLOT_HEAD);
}

/// Identity-root bind world: `evaluate(from_bind)` equals composing each bone's
/// current `BIND_*` local onto its parent world. Topology (parent indices) is
/// frozen. Rest translations are **tunable** — if `BIND_*` is retuned, update
/// this test; do not treat the numbers as an architecture freeze.
#[test]
fn humanoid_v0_bind_world_matches_composed_bind_locals() {
    let def = humanoid_v0();
    assert_eq!(def.parent(ROOT), None);
    assert_eq!(def.parent(PELVIS), Some(ROOT));
    assert_eq!(def.parent(TORSO), Some(PELVIS));
    assert_eq!(def.parent(HEAD), Some(TORSO));
    assert_eq!(def.parent(UPPER_ARM_FRONT), Some(TORSO));
    assert_eq!(def.parent(LOWER_ARM_FRONT), Some(UPPER_ARM_FRONT));
    assert_eq!(def.parent(HAND_FRONT), Some(LOWER_ARM_FRONT));
    assert_eq!(def.parent(UPPER_ARM_BACK), Some(TORSO));
    assert_eq!(def.parent(LOWER_ARM_BACK), Some(UPPER_ARM_BACK));
    assert_eq!(def.parent(HAND_BACK), Some(LOWER_ARM_BACK));
    assert_eq!(def.parent(UPPER_LEG_FRONT), Some(PELVIS));
    assert_eq!(def.parent(LOWER_LEG_FRONT), Some(UPPER_LEG_FRONT));
    assert_eq!(def.parent(FOOT_FRONT), Some(LOWER_LEG_FRONT));
    assert_eq!(def.parent(UPPER_LEG_BACK), Some(PELVIS));
    assert_eq!(def.parent(LOWER_LEG_BACK), Some(UPPER_LEG_BACK));
    assert_eq!(def.parent(FOOT_BACK), Some(LOWER_LEG_BACK));

    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();

    assert_tr_eq(world.get(ROOT).unwrap(), BIND_ROOT);

    for i in 0..def.bone_count() {
        let idx = BoneIndex::from_u8(u8::try_from(i).unwrap());
        let bind_local = local.get(idx).unwrap();
        let expected = match def.parent(idx) {
            None => bind_local,
            Some(parent) => world.get(parent).unwrap().compose(bind_local),
        };
        assert_tr_eq(world.get(idx).unwrap(), expected);
    }

    let pelvis = BIND_ROOT.compose(BIND_PELVIS);
    let torso = pelvis.compose(BIND_TORSO);
    let head = torso.compose(BIND_HEAD);
    assert_tr_eq(world.get(PELVIS).unwrap(), pelvis);
    assert_tr_eq(world.get(TORSO).unwrap(), torso);
    assert_tr_eq(world.get(HEAD).unwrap(), head);

    let upper_front = pelvis.compose(BIND_UPPER_LEG_FRONT);
    let shin_front = upper_front.compose(BIND_LOWER_LEG_FRONT);
    let foot_front = shin_front.compose(BIND_FOOT_FRONT);
    assert_tr_eq(world.get(UPPER_LEG_FRONT).unwrap(), upper_front);
    assert_tr_eq(world.get(LOWER_LEG_FRONT).unwrap(), shin_front);
    assert_tr_eq(world.get(FOOT_FRONT).unwrap(), foot_front);
    let upper_back = pelvis.compose(BIND_UPPER_LEG_BACK);
    let shin_back = upper_back.compose(BIND_LOWER_LEG_BACK);
    let foot_back = shin_back.compose(BIND_FOOT_BACK);
    assert_tr_eq(world.get(UPPER_LEG_BACK).unwrap(), upper_back);
    assert_tr_eq(world.get(LOWER_LEG_BACK).unwrap(), shin_back);
    assert_tr_eq(world.get(FOOT_BACK).unwrap(), foot_back);

    let hip_dy = pelvis.translation[1] - upper_front.translation[1];
    let thigh_dy = upper_front.translation[1] - shin_front.translation[1];
    let shin_dx = foot_front.translation[0] - shin_front.translation[0];
    let shin_dy = shin_front.translation[1] - foot_front.translation[1];
    assert!(
        hip_dy > 0.02 && hip_dy < 0.08,
        "upper_leg_front vertical {hip_dy} is a hip placement offset, not the thigh span"
    );
    assert!(
        thigh_dy > 0.18,
        "lower_leg_front vertical {thigh_dy} is the hip→knee thigh span"
    );
    assert!(
        shin_dx.abs() < EPS,
        "foot_front bind x {shin_dx} must keep knee→ankle vertical; forward length is the foot visual"
    );
    assert!(
        shin_dy > 0.16,
        "foot_front vertical {shin_dy} is the knee→ankle shin span"
    );
    assert!(
        foot_front.translation[1].abs() < 0.02,
        "foot_front world y {} should stay near the planted root",
        foot_front.translation[1]
    );
    assert!(
        foot_back.translation[1].abs() < 0.02,
        "foot_back world y {} should stay near the planted root",
        foot_back.translation[1]
    );
    let back_shin_dx = foot_back.translation[0] - shin_back.translation[0];
    assert!(
        back_shin_dx.abs() < EPS,
        "foot_back bind x {back_shin_dx} must keep the back shin vertical"
    );
}

#[test]
fn humanoid_v0_hip_bind_x_is_widened_neutral_stance() {
    assert!((BIND_UPPER_LEG_FRONT.translation[0] + 0.06).abs() < EPS);
    assert!((BIND_UPPER_LEG_BACK.translation[0] - 0.04).abs() < EPS);
    assert!((BIND_UPPER_LEG_FRONT.translation[1] + 0.04).abs() < EPS);
    assert!((BIND_UPPER_LEG_BACK.translation[1] + 0.04).abs() < EPS);
    assert!(BIND_UPPER_LEG_FRONT.rotation.abs() < EPS);
    assert!(BIND_UPPER_LEG_BACK.rotation.abs() < EPS);
    assert!(BIND_LOWER_LEG_FRONT.translation[0].abs() < EPS);
    assert!(BIND_LOWER_LEG_BACK.translation[0].abs() < EPS);
    assert!(BIND_FOOT_FRONT.translation[0].abs() < EPS);
    assert!(BIND_FOOT_BACK.translation[0].abs() < EPS);
    let sep = BIND_UPPER_LEG_BACK.translation[0] - BIND_UPPER_LEG_FRONT.translation[0];
    assert!((sep - 0.10).abs() < EPS);
}

#[test]
fn humanoid_v0_shoulder_bind_x_is_three_quarter_right() {
    assert!((BIND_UPPER_ARM_FRONT.translation[0] + 0.10).abs() < EPS);
    assert!((BIND_UPPER_ARM_BACK.translation[0] - 0.08).abs() < EPS);
    assert!((BIND_UPPER_ARM_FRONT.translation[1] - 0.14).abs() < EPS);
    assert!((BIND_UPPER_ARM_BACK.translation[1] - 0.14).abs() < EPS);
    assert!(BIND_UPPER_ARM_FRONT.rotation.abs() < EPS);
    assert!(BIND_UPPER_ARM_BACK.rotation.abs() < EPS);
    assert!(BIND_LOWER_ARM_FRONT.translation[0].abs() < EPS);
    assert!((BIND_LOWER_ARM_FRONT.translation[1] + 0.22).abs() < EPS);
    assert!(BIND_LOWER_ARM_BACK.translation[0].abs() < EPS);
    assert!((BIND_LOWER_ARM_BACK.translation[1] + 0.22).abs() < EPS);
    assert!((BIND_LOWER_ARM_BACK.rotation - 0.50).abs() < EPS);
    assert!(BIND_HAND_BACK.rotation.abs() < EPS);
    let sep = BIND_UPPER_ARM_BACK.translation[0] - BIND_UPPER_ARM_FRONT.translation[0];
    assert!((sep - 0.18).abs() < EPS);
}

#[test]
fn front_back_arm_bind_x_swap_keeps_semantic_chains() {
    let def = humanoid_v0();
    assert_eq!(
        humanoid_v0_bone_by_label("upper_arm_front"),
        Some(UPPER_ARM_FRONT)
    );
    assert_eq!(
        humanoid_v0_bone_by_label("upper_arm_back"),
        Some(UPPER_ARM_BACK)
    );
    assert_eq!(def.parent(UPPER_ARM_FRONT), Some(TORSO));
    assert_eq!(def.parent(LOWER_ARM_FRONT), Some(UPPER_ARM_FRONT));
    assert_eq!(def.parent(HAND_FRONT), Some(LOWER_ARM_FRONT));
    assert_eq!(def.parent(UPPER_ARM_BACK), Some(TORSO));
    assert_eq!(def.parent(LOWER_ARM_BACK), Some(UPPER_ARM_BACK));
    assert_eq!(def.parent(HAND_BACK), Some(LOWER_ARM_BACK));

    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let torso = world.get(TORSO).unwrap();
    let front_shoulder = world.get(UPPER_ARM_FRONT).unwrap();
    let back_shoulder = world.get(UPPER_ARM_BACK).unwrap();
    assert!((front_shoulder.translation[0] - (torso.translation[0] - 0.10)).abs() < EPS);
    assert!((back_shoulder.translation[0] - (torso.translation[0] + 0.08)).abs() < EPS);
    let front_hand = world.get(HAND_FRONT).unwrap();
    let front_elbow = world.get(LOWER_ARM_FRONT).unwrap();
    assert!((front_hand.translation[0] - front_elbow.translation[0]).abs() < EPS);
    assert!(front_hand.translation[1] < front_elbow.translation[1]);
    let back_hand = world.get(HAND_BACK).unwrap();
    let back_elbow = world.get(LOWER_ARM_BACK).unwrap();
    let expected_back_hand = back_elbow.compose(BIND_HAND_BACK);
    assert_tr_eq(back_hand, expected_back_hand);
}

#[test]
fn torso_placeholder_is_authored_in_torso_bone_local() {
    let hip_y = BIND_UPPER_LEG_FRONT.translation[1] - BIND_TORSO.translation[1];
    let top = BIND_HEAD.translation[1];
    let corners = torso_local_corners(1.0);
    let far = torso_far_local_corners(1.0);
    assert!((corners[0][1] - hip_y).abs() < EPS);
    assert!((corners[2][1] - top).abs() < EPS);
    assert!((far[0][1] - hip_y).abs() < EPS);
    assert!((far[2][1] - top).abs() < EPS);
    assert!(corners[0][1] < 0.0);
    assert!(corners[2][1] > 0.0);
    let layout = torso_placeholder_layout();
    assert!((layout.height - (top - hip_y)).abs() < EPS);
    assert!(
        layout.local_center[0].abs() < 1e-4,
        "torso visual centroid X must sit on the torso origin, got {}",
        layout.local_center[0]
    );
    let xs = [corners[0][0], corners[1][0], corners[2][0], corners[3][0]];
    let mean_x = (xs[0] + xs[1] + xs[2] + xs[3]) * 0.25;
    assert!(mean_x.abs() < 1e-4);
    assert!(
        corners[2][0] > corners[3][0].abs(),
        "keep 3/4 near-side (+X) exposure at the top"
    );
}

#[test]
fn humanoid_v0_head_bind_leads_slightly_right() {
    assert!((BIND_HEAD.translation[0] - 0.07).abs() < EPS);
    assert!((BIND_HEAD.translation[1] - 0.24).abs() < EPS);
    assert!(BIND_HEAD.rotation.abs() < EPS);
}

#[test]
fn humanoid_v0_labels_match_indices_and_reject_unknown() {
    assert_eq!(HUMANOID_V0_BONE_LABELS.len(), humanoid_v0().bone_count());
    assert_eq!(HUMANOID_V0_SLOT_LABELS.len(), humanoid_v0().slot_count());
    assert_eq!(HUMANOID_V0_SLOT_LABELS[SLOT_HEAD.as_usize()], "head");
    assert_eq!(HUMANOID_V0_SLOT_LABELS[SLOT_WEAPON.as_usize()], "weapon");
    assert_eq!(
        HUMANOID_V0_SLOT_LABELS[SLOT_SHOE_FRONT.as_usize()],
        "shoe_front"
    );
    assert_eq!(humanoid_v0_bone_by_label("root"), Some(ROOT));
    assert_eq!(humanoid_v0_bone_by_label("pelvis"), Some(PELVIS));
    assert_eq!(humanoid_v0_bone_by_label("head"), Some(HEAD));
    assert_eq!(humanoid_v0_bone_by_label("torso"), Some(TORSO));
    assert_eq!(humanoid_v0_bone_by_label("hand_front"), Some(HAND_FRONT));
    assert_eq!(humanoid_v0_bone_by_label("foot_back"), Some(FOOT_BACK));
    assert!(humanoid_v0_bone_by_label("not_a_bone").is_none());
    assert!(humanoid_v0_bone_by_label("Root").is_none());
}

#[test]
fn presentation_anchors_are_derived_from_humanoid_v0_bind() {
    assert_tr_eq(
        ANCHOR_CROWN,
        BoneTransform::from_translation_rotation([0.0, BIND_HEAD.translation[1]], 0.0),
    );
    assert_tr_eq(
        ANCHOR_CHEST,
        BoneTransform::from_translation_rotation(
            [
                BIND_HEAD.translation[0] * 0.5,
                BIND_HEAD.translation[1] * 0.5,
            ],
            0.0,
        ),
    );
    assert_tr_eq(ANCHOR_GRIP, SLOT_REST_WEAPON);
    assert_tr_eq(ANCHOR_FOOT, SLOT_REST_SHOE_FRONT);
}

#[test]
fn skeleton_manifest_has_no_gameplay_or_network_deps() {
    let manifest = include_str!("../Cargo.toml");
    for forbidden in [
        "purgatory-simulation",
        "purgatory-protocol",
        "purgatory-client",
        "purgatory-server",
        "purgatory-content",
        "purgatory-common",
        "wgpu",
        "winit",
        "egui",
        "quinn",
        "tokio",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "purgatory-skeleton must not depend on {forbidden}"
        );
    }
}
