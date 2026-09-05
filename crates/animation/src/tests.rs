//! Focused A1 sampling and A2 playback tests.

use core::f32::consts::PI;

use purgatory_skeleton::{
    BIND_HEAD, HEAD, LOWER_ARM_FRONT, LOWER_LEG_FRONT, LocalPose, PELVIS, ROOT, TORSO,
    UPPER_ARM_BACK, UPPER_ARM_FRONT, UPPER_LEG_BACK, UPPER_LEG_FRONT, WorldPose, evaluate,
    humanoid_v0,
};

use crate::blend::{BlendError, blend_local_poses};
use crate::clip::{AnimationClip, BoneTrack, ClipError, Interpolation, Keyframe, LoopPolicy};
use crate::dev::{
    A1_HEAD_CLIP_DURATION, A5_ATTACK_CLIP_DURATION, A5_HURT_CLIP_DURATION, a1_head_loop_clip,
    a1_head_rotation_clip, a3_idle_clip, a3_move_clip, a4_fall_clip, a4_jump_clip, a5_attack_clip,
    a5_hurt_clip, climb_back_clip,
};
use crate::parse_animation_asset_v1;
use crate::player::{AnimationPlayer, PlayerError};
use crate::sample::{SampleError, sample, shortest_angle_delta};

const EPS: f32 = 1e-5;

fn head_clip(keys: Vec<Keyframe>) -> AnimationClip {
    let def = humanoid_v0();
    AnimationClip::try_new(
        def,
        A1_HEAD_CLIP_DURATION,
        LoopPolicy::Once,
        vec![BoneTrack::rotation_only(HEAD, keys)],
    )
    .expect("valid head clip")
}

fn linear_keys(times_values: &[(f32, f32)]) -> Vec<Keyframe> {
    times_values
        .iter()
        .map(|(t, v)| Keyframe {
            time: *t,
            value: *v,
            interpolation: Interpolation::Linear,
        })
        .collect()
}

#[test]
fn exact_key_sampling() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(clip, 0.0, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.0).abs() < EPS);
    sample(clip, 0.5, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.40).abs() < EPS);
    sample(clip, 1.0, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - (-0.40)).abs() < EPS);
}

#[test]
fn midpoint_linear_interpolation() {
    let clip = head_clip(linear_keys(&[(0.0, 0.0), (1.0, 0.80)]));
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(&clip, 0.5, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.40).abs() < EPS);
}

#[test]
fn shortest_angle_across_pi() {
    let from = 0.95 * PI;
    let to = -0.95 * PI;
    let delta = shortest_angle_delta(from, to);
    assert!(
        (delta - 0.1 * PI).abs() < 1e-4,
        "expected short +0.1π, got {delta}"
    );
    let raw = to - from;
    assert!(raw.abs() > PI, "raw delta must be the long way");

    let clip = head_clip(linear_keys(&[(0.0, from), (1.0, to)]));
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(&clip, 0.5, &mut local).unwrap();
    let mid = local.get(HEAD).unwrap().rotation;
    let expected = from + delta * 0.5;
    assert!(
        (mid - expected).abs() < 1e-4,
        "mid={mid} expected={expected} (raw mid would be ~0)"
    );
    assert!(
        mid.abs() > 2.0,
        "must not take the long-way midpoint near 0"
    );
}

#[test]
fn hold_before_first_and_after_last() {
    let clip = head_clip(linear_keys(&[(0.2, 0.10), (0.8, 0.50)]));
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(&clip, -1.0, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.10).abs() < EPS);
    sample(&clip, 0.0, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.10).abs() < EPS);
    sample(&clip, 2.0, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.50).abs() < EPS);
}

#[test]
fn construction_rejects_invalid_data() {
    let def = humanoid_v0();
    let ok_keys = linear_keys(&[(0.0, 0.0), (1.0, 0.2)]);

    assert!(matches!(
        AnimationClip::try_new(
            def,
            f32::NAN,
            LoopPolicy::Once,
            vec![BoneTrack::rotation_only(HEAD, ok_keys.clone())]
        ),
        Err(ClipError::NonFiniteDuration)
    ));
    assert!(matches!(
        AnimationClip::try_new(
            def,
            0.0,
            LoopPolicy::Once,
            vec![BoneTrack::rotation_only(HEAD, ok_keys.clone())]
        ),
        Err(ClipError::NonPositiveDuration)
    ));
    assert!(matches!(
        AnimationClip::try_new(
            def,
            1.0,
            LoopPolicy::Once,
            vec![BoneTrack::rotation_only(
                HEAD,
                linear_keys(&[(0.5, 0.0), (0.2, 0.1)])
            )]
        ),
        Err(ClipError::UnsortedKeyTimes { .. })
    ));
    assert!(matches!(
        AnimationClip::try_new(
            def,
            1.0,
            LoopPolicy::Once,
            vec![BoneTrack::rotation_only(
                HEAD,
                linear_keys(&[(0.5, 0.0), (0.5, 0.1)])
            )]
        ),
        Err(ClipError::DuplicateKeyTime { .. })
    ));
    assert!(matches!(
        AnimationClip::try_new(
            def,
            1.0,
            LoopPolicy::Once,
            vec![
                BoneTrack::rotation_only(HEAD, ok_keys.clone()),
                BoneTrack::rotation_only(HEAD, ok_keys.clone()),
            ]
        ),
        Err(ClipError::DuplicateBoneTrack { .. })
    ));
    let bad_bone = purgatory_skeleton::BoneIndex::from_u8(200);
    assert!(matches!(
        AnimationClip::try_new(
            def,
            1.0,
            LoopPolicy::Once,
            vec![BoneTrack::rotation_only(bad_bone, ok_keys)]
        ),
        Err(ClipError::BoneOutOfRange { .. })
    ));
}

#[test]
fn dense_bone_index_targets_head() {
    let clip = a1_head_rotation_clip();
    assert_eq!(clip.tracks().len(), 1);
    assert_eq!(clip.tracks()[0].bone, HEAD);
    assert!(clip.tracks()[0].rotation.is_some());
    assert!(clip.tracks()[0].translation_x.is_none());
    assert!(clip.tracks()[0].translation_y.is_none());
}

#[test]
fn unkeyed_bones_remain_bind() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(clip, 0.5, &mut local).unwrap();
    assert_eq!(local.get(TORSO).unwrap(), def.bind_local(TORSO).unwrap());
    assert_eq!(local.get(ROOT).unwrap(), def.bind_local(ROOT).unwrap());
}

#[test]
fn head_rotation_changes_translation_stays_bind() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(clip, 0.5, &mut local).unwrap();
    let head = local.get(HEAD).unwrap();
    assert!((head.rotation - 0.40).abs() < EPS);
    assert!((head.translation[0] - BIND_HEAD.translation[0]).abs() < EPS);
    assert!((head.translation[1] - BIND_HEAD.translation[1]).abs() < EPS);
}

#[test]
fn root_unchanged_by_sample() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let before = local.get(ROOT).unwrap();
    sample(clip, 0.75, &mut local).unwrap();
    assert_eq!(local.get(ROOT).unwrap(), before);
}

#[test]
fn sampled_pose_evaluates() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    sample(clip, 0.5, &mut local).unwrap();
    evaluate(def, &local, &mut world).unwrap();
    assert!(world.get(HEAD).unwrap().rotation.is_finite());
}

#[test]
fn sample_error_leaves_local_pose_unchanged() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(clip, 0.5, &mut local).unwrap();
    let snapshot: Vec<_> = local.as_slice().to_vec();

    assert_eq!(
        sample(clip, f32::NAN, &mut local),
        Err(SampleError::NonFiniteTime)
    );
    assert_eq!(local.as_slice(), snapshot.as_slice());

    let mut short = LocalPose::with_identity(4);
    let before = short.as_slice().to_vec();
    assert!(matches!(
        sample(clip, 0.0, &mut short),
        Err(SampleError::BoneCountMismatch { .. })
    ));
    assert_eq!(short.as_slice(), before.as_slice());
}

#[test]
fn sample_reuses_caller_owned_buffer() {
    let clip = a1_head_rotation_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let len0 = local.bone_count();
    let ptr0 = local.as_slice().as_ptr();
    sample(clip, 0.25, &mut local).unwrap();
    sample(clip, 0.75, &mut local).unwrap();
    assert_eq!(local.bone_count(), len0);
    assert_eq!(local.as_slice().as_ptr(), ptr0);
}

#[test]
fn animation_manifest_has_no_gameplay_or_network_deps() {
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
        "serde",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "purgatory-animation must not depend on {forbidden}"
        );
    }
    assert!(
        manifest.contains("purgatory-skeleton"),
        "purgatory-animation must depend on purgatory-skeleton"
    );
}

const PLAYER_EPS: f32 = 1e-5;

#[test]
fn player_starts_paused_at_zero() {
    let player = AnimationPlayer::new();
    assert!(!player.playing());
    assert!((player.speed() - 1.0).abs() < PLAYER_EPS);
    assert!((player.sample_time(a1_head_loop_clip()) - 0.0).abs() < PLAYER_EPS);
}

#[test]
fn paused_advance_is_noop() {
    let mut player = AnimationPlayer::new();
    player.advance(0.5, a1_head_loop_clip()).unwrap();
    assert!((player.sample_time(a1_head_loop_clip()) - 0.0).abs() < PLAYER_EPS);
}

#[test]
fn once_clamps_sample_time_at_duration() {
    let clip = a1_head_rotation_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.5, clip).unwrap();
    assert!((player.sample_time(clip) - 0.5).abs() < PLAYER_EPS);
    player.advance(2.0, clip).unwrap();
    assert!((player.sample_time(clip) - A1_HEAD_CLIP_DURATION).abs() < PLAYER_EPS);
}

#[test]
fn loop_exact_duration_resolves_to_zero() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(1.0, clip).unwrap();
    assert_eq!(player.sample_time(clip), 0.0);
}

#[test]
fn loop_large_dt_wraps() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(3.25, clip).unwrap();
    assert!((player.sample_time(clip) - 0.25).abs() < PLAYER_EPS);
}

#[test]
fn loop_fractional_advances_use_tolerance() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    for _ in 0..4 {
        player.advance(0.25, clip).unwrap();
    }
    assert!(
        (player.sample_time(clip) - 0.0).abs() < 1e-4,
        "four × 0.25 should wrap near 0, got {}",
        player.sample_time(clip)
    );
}

#[test]
fn sample_time_is_idempotent() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.37, clip).unwrap();
    let a = player.sample_time(clip);
    let b = player.sample_time(clip);
    assert_eq!(a, b);
}

#[test]
fn speed_scales_inside_advance() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.set_speed(2.0).unwrap();
    player.advance(0.25, clip).unwrap();
    assert!((player.sample_time(clip) - 0.5).abs() < PLAYER_EPS);
}

#[test]
fn zero_speed_is_valid() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.3, clip).unwrap();
    player.set_speed(0.0).unwrap();
    let before = player.sample_time(clip);
    player.advance(1.0, clip).unwrap();
    assert_eq!(player.sample_time(clip), before);
}

#[test]
fn set_speed_does_not_change_elapsed_or_playing() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.4, clip).unwrap();
    let t = player.sample_time(clip);
    player.set_speed(0.5).unwrap();
    assert!(player.playing());
    assert_eq!(player.sample_time(clip), t);
}

#[test]
fn reset_clears_elapsed_keeps_playing_and_speed() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.set_speed(1.5).unwrap();
    player.advance(0.8, clip).unwrap();
    player.reset();
    assert!(player.playing());
    assert!((player.speed() - 1.5).abs() < PLAYER_EPS);
    assert_eq!(player.sample_time(clip), 0.0);
}

#[test]
fn invalid_dt_leaves_state_unchanged() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.2, clip).unwrap();
    let t = player.sample_time(clip);
    assert_eq!(player.advance(-0.1, clip), Err(PlayerError::NegativeDt));
    assert_eq!(
        player.advance(f32::NAN, clip),
        Err(PlayerError::NonFiniteDt)
    );
    assert_eq!(player.sample_time(clip), t);
    assert!(player.playing());
}

#[test]
fn invalid_speed_leaves_state_unchanged() {
    let clip = a1_head_loop_clip();
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.2, clip).unwrap();
    let t = player.sample_time(clip);
    assert_eq!(player.set_speed(-1.0), Err(PlayerError::NegativeSpeed));
    assert_eq!(player.set_speed(f32::NAN), Err(PlayerError::NonFiniteSpeed));
    assert!((player.speed() - 1.0).abs() < PLAYER_EPS);
    assert_eq!(player.sample_time(clip), t);
}

#[test]
fn player_loop_integrates_with_sample() {
    let clip = a1_head_loop_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let mut player = AnimationPlayer::new();
    player.set_playing(true);
    player.advance(0.5, clip).unwrap();
    let t = player.sample_time(clip);
    sample(clip, t, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - 0.40).abs() < PLAYER_EPS);
}

#[test]
fn loop_fixture_matches_once_keys_at_mid() {
    let once = a1_head_rotation_clip();
    let looping = a1_head_loop_clip();
    assert_eq!(once.loop_policy(), LoopPolicy::Once);
    assert_eq!(looping.loop_policy(), LoopPolicy::Loop);
    let def = humanoid_v0();
    let mut a = LocalPose::from_bind(def);
    let mut b = LocalPose::from_bind(def);
    sample(once, 0.5, &mut a).unwrap();
    sample(looping, 0.5, &mut b).unwrap();
    assert_eq!(a.get(HEAD).unwrap().rotation, b.get(HEAD).unwrap().rotation);
}

#[test]
fn a3_idle_clip_samples_and_evaluates() {
    let clip = a3_idle_clip();
    assert_eq!(clip.loop_policy(), LoopPolicy::Loop);
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let bind_head = local.get(HEAD).unwrap().rotation;
    let bind_root = local.get(ROOT).unwrap();
    sample(clip, 0.5, &mut local).unwrap();
    assert!((local.get(HEAD).unwrap().rotation - bind_head).abs() > 1e-3);
    assert!((local.get(PELVIS).unwrap().rotation - 0.0).abs() > 1e-4);
    assert_eq!(local.get(ROOT).unwrap(), bind_root);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    assert!(world.get(HEAD).unwrap().rotation.is_finite());
}

#[test]
fn climb_back_clip_samples_pelvis_tx_not_idle() {
    let clip = climb_back_clip();
    let idle = a3_idle_clip();
    let def = humanoid_v0();
    let authored = parse_animation_asset_v1(
        "climb_back.anim",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/shared/animations/dev/climb_back.anim"
        )),
        def,
    )
    .expect("authored climb_back.anim");
    assert_eq!(clip.loop_policy(), LoopPolicy::Loop);
    assert!((clip.duration() - authored.clip.duration()).abs() < 1e-4);
    assert!(!std::ptr::eq(clip, idle));
    let mut climb_local = LocalPose::from_bind(def);
    let mut idle_local = LocalPose::from_bind(def);
    let bind_tx = climb_local.get(PELVIS).unwrap().translation[0];
    let sample_t = clip
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
        .expect("authored climb_back pelvis tx leaves bind");
    sample(clip, sample_t, &mut climb_local).unwrap();
    sample(idle, sample_t, &mut idle_local).unwrap();
    assert!((climb_local.get(PELVIS).unwrap().translation[0] - bind_tx).abs() > 0.1);
    assert!(
        (climb_local.get(PELVIS).unwrap().translation[0]
            - idle_local.get(PELVIS).unwrap().translation[0])
            .abs()
            > 0.1
    );
    assert_eq!(
        climb_local.get(ROOT).unwrap().translation,
        idle_local.get(ROOT).unwrap().translation
    );
    let mut world = WorldPose::new(def);
    evaluate(def, &climb_local, &mut world).unwrap();
    assert!(world.get(PELVIS).unwrap().translation[0].is_finite());
}

#[test]
fn a3_move_clip_samples_limbs_and_evaluates() {
    let clip = a3_move_clip();
    assert_eq!(clip.loop_policy(), LoopPolicy::Loop);
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let bind_leg = local.get(UPPER_LEG_FRONT).unwrap().rotation;
    let bind_root = local.get(ROOT).unwrap();
    sample(clip, 0.2, &mut local).unwrap();
    assert!((local.get(UPPER_LEG_FRONT).unwrap().rotation - bind_leg).abs() > 0.1);
    assert!((local.get(UPPER_ARM_BACK).unwrap().rotation).abs() > 0.05);
    assert_eq!(local.get(ROOT).unwrap(), bind_root);
    assert_eq!(
        local.get(HEAD).unwrap().rotation,
        def.bind_local(HEAD).unwrap().rotation
    );
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
}

#[test]
fn a4_jump_and_fall_clips_distinct_and_evaluate() {
    let jump = a4_jump_clip();
    let fall = a4_fall_clip();
    let def = humanoid_v0();
    let mut j = LocalPose::from_bind(def);
    let mut f = LocalPose::from_bind(def);
    let root = j.get(ROOT).unwrap();
    sample(jump, 0.0, &mut j).unwrap();
    sample(fall, 0.0, &mut f).unwrap();
    assert_eq!(j.get(ROOT).unwrap(), root);
    assert_eq!(f.get(ROOT).unwrap(), root);
    let j_arm = j.get(UPPER_ARM_FRONT).unwrap().rotation;
    let f_arm = f.get(UPPER_ARM_FRONT).unwrap().rotation;
    assert!(j_arm > 0.5);
    assert!(f_arm < -0.5);
    assert!((j_arm - f_arm).abs() > 1.0);
    let mut world = WorldPose::new(def);
    evaluate(def, &j, &mut world).unwrap();
    evaluate(def, &f, &mut world).unwrap();
}

/// Skeleton plane rotation is CCW with +Y up. For a RIGHT-facing humanoid, a
/// positive local rotation on a downward-hanging limb moves the distal child
/// toward +X (facing-forward / screen-right). Front/Back identities do not swap.
#[test]
fn right_facing_positive_rotation_moves_hanging_limb_toward_plus_x() {
    let def = humanoid_v0();
    let bind = LocalPose::from_bind(def);
    let mut pos = LocalPose::from_bind(def);
    let mut neg = LocalPose::from_bind(def);
    pos.get_mut(UPPER_ARM_FRONT).unwrap().rotation = 0.6;
    neg.get_mut(UPPER_ARM_FRONT).unwrap().rotation = -0.6;
    pos.get_mut(UPPER_LEG_FRONT).unwrap().rotation = 0.6;
    neg.get_mut(UPPER_LEG_FRONT).unwrap().rotation = -0.6;

    let mut bind_w = WorldPose::new(def);
    let mut pos_w = WorldPose::new(def);
    let mut neg_w = WorldPose::new(def);
    evaluate(def, &bind, &mut bind_w).unwrap();
    evaluate(def, &pos, &mut pos_w).unwrap();
    evaluate(def, &neg, &mut neg_w).unwrap();

    let arm_bind_x = bind_w.get(LOWER_ARM_FRONT).unwrap().translation[0];
    let arm_pos_x = pos_w.get(LOWER_ARM_FRONT).unwrap().translation[0];
    let arm_neg_x = neg_w.get(LOWER_ARM_FRONT).unwrap().translation[0];
    assert!(
        arm_pos_x > arm_bind_x + 0.05,
        "positive upper_arm_front must move forearm toward +X (got bind={arm_bind_x}, +={arm_pos_x})"
    );
    assert!(
        arm_neg_x < arm_bind_x - 0.05,
        "negative upper_arm_front must move forearm toward -X (got bind={arm_bind_x}, -={arm_neg_x})"
    );

    let leg_bind_x = bind_w.get(LOWER_LEG_FRONT).unwrap().translation[0];
    let leg_pos_x = pos_w.get(LOWER_LEG_FRONT).unwrap().translation[0];
    let leg_neg_x = neg_w.get(LOWER_LEG_FRONT).unwrap().translation[0];
    assert!(
        leg_pos_x > leg_bind_x + 0.05,
        "positive upper_leg_front must move shin toward +X (got bind={leg_bind_x}, +={leg_pos_x})"
    );
    assert!(
        leg_neg_x < leg_bind_x - 0.05,
        "negative upper_leg_front must move shin toward -X (got bind={leg_bind_x}, -={leg_neg_x})"
    );

    // Sanity: Front/Back indices stay distinct bones (no swap under facing).
    assert_ne!(UPPER_ARM_FRONT, UPPER_ARM_BACK);
    assert_ne!(UPPER_LEG_FRONT, UPPER_LEG_BACK);
}

/// Attack mid-swing must throw the near arm toward +X for Facing::Right authoring.
#[test]
fn a5_attack_mid_swings_front_arm_toward_plus_x() {
    let clip = a5_attack_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    sample(clip, A5_ATTACK_CLIP_DURATION * 0.45, &mut local).unwrap();
    let arm_r = local.get(UPPER_ARM_FRONT).unwrap().rotation;
    assert!(
        arm_r > 1.0,
        "Attack mid must use positive front-arm rotation toward +X, got {arm_r}"
    );

    let mut bind_w = WorldPose::new(def);
    let mut atk_w = WorldPose::new(def);
    let bind = LocalPose::from_bind(def);
    evaluate(def, &bind, &mut bind_w).unwrap();
    evaluate(def, &local, &mut atk_w).unwrap();
    let dx = atk_w.get(LOWER_ARM_FRONT).unwrap().translation[0]
        - bind_w.get(LOWER_ARM_FRONT).unwrap().translation[0];
    assert!(
        dx > 0.1,
        "Attack mid forearm world X must increase vs bind (Δx={dx})"
    );
}

/// Move gait for Facing::Right: near-leg forward (+X) pairs with near-arm back (−X).
#[test]
fn a3_move_right_facing_opposite_arm_leg_phase() {
    let clip = a3_move_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    // q1 = 0.25 * duration = 0.2s — first swing extremum.
    sample(clip, 0.2, &mut local).unwrap();
    let leg = local.get(UPPER_LEG_FRONT).unwrap().rotation;
    let arm = local.get(UPPER_ARM_FRONT).unwrap().rotation;
    assert!(
        leg > 0.2,
        "Move q1 near-leg must swing forward (+), got {leg}"
    );
    assert!(
        arm < -0.2,
        "Move q1 near-arm must swing back (−), got {arm}"
    );
}

#[test]
fn a5_attack_clip_preserves_unkeyed_bones_at_bind() {
    let clip = a5_attack_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let bind_root_rot = local.get(ROOT).unwrap().rotation;
    let bind_leg_back_rot = local.get(UPPER_LEG_FRONT).unwrap().rotation; // unkeyed in A5 attack
    sample(clip, A5_ATTACK_CLIP_DURATION * 0.5, &mut local).unwrap();
    assert!((local.get(ROOT).unwrap().rotation - bind_root_rot).abs() < EPS);
    assert!(
        (local.get(UPPER_LEG_FRONT).unwrap().rotation - bind_leg_back_rot).abs() < EPS,
        "unkeyed upper_leg_front must remain bind-rotation"
    );
}

#[test]
fn a5_hurt_clip_preserves_unkeyed_bones_at_bind() {
    let clip = a5_hurt_clip();
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let bind_root_rot = local.get(ROOT).unwrap().rotation;
    let bind_leg_back_rot = local.get(UPPER_LEG_BACK).unwrap().rotation; // unkeyed in A5 hurt
    sample(clip, A5_HURT_CLIP_DURATION * 0.5, &mut local).unwrap();
    assert!((local.get(ROOT).unwrap().rotation - bind_root_rot).abs() < EPS);
    assert!(
        (local.get(UPPER_LEG_BACK).unwrap().rotation - bind_leg_back_rot).abs() < EPS,
        "unkeyed upper_leg_back must remain bind-rotation"
    );
}

#[test]
fn blend_local_poses_shortest_angle_and_endpoints() {
    let def = humanoid_v0();
    let mut from = LocalPose::from_bind(def);
    let mut to = LocalPose::from_bind(def);
    let mut out = LocalPose::from_bind(def);
    from.get_mut(HEAD).unwrap().rotation = 0.95 * PI;
    to.get_mut(HEAD).unwrap().rotation = -0.95 * PI;
    blend_local_poses(&from, &to, 0.5, &mut out).unwrap();
    let mid = out.get(HEAD).unwrap().rotation;
    let expected = from.get(HEAD).unwrap().rotation
        + shortest_angle_delta(
            from.get(HEAD).unwrap().rotation,
            to.get(HEAD).unwrap().rotation,
        ) * 0.5;
    assert!((mid - expected).abs() < 1e-4);
    assert!(mid.abs() > 2.0);

    blend_local_poses(&from, &to, 0.0, &mut out).unwrap();
    assert_eq!(
        out.get(HEAD).unwrap().rotation,
        from.get(HEAD).unwrap().rotation
    );
    blend_local_poses(&from, &to, 1.0, &mut out).unwrap();
    assert_eq!(
        out.get(HEAD).unwrap().rotation,
        to.get(HEAD).unwrap().rotation
    );
    assert_eq!(
        blend_local_poses(&from, &to, f32::NAN, &mut out),
        Err(BlendError::NonFiniteAlpha)
    );
}

#[test]
fn blend_preserves_unkeyed_and_root_copy() {
    let def = humanoid_v0();
    let mut from = LocalPose::from_bind(def);
    let mut to = LocalPose::from_bind(def);
    let mut out = LocalPose::from_bind(def);
    from.get_mut(ROOT).unwrap().translation = [1.0, 2.0];
    to.get_mut(ROOT).unwrap().translation = [3.0, 4.0];
    to.get_mut(HEAD).unwrap().rotation = 0.5;
    blend_local_poses(&from, &to, 0.5, &mut out).unwrap();
    assert!((out.get(ROOT).unwrap().translation[0] - 2.0).abs() < 1e-5);
    assert!((out.get(HEAD).unwrap().rotation - 0.25).abs() < 1e-5);
    assert_eq!(
        out.get(TORSO).unwrap().rotation,
        def.bind_local(TORSO).unwrap().rotation
    );
}

fn depth_clip(keys: Vec<Keyframe>) -> AnimationClip {
    let def = humanoid_v0();
    AnimationClip::try_new(
        def,
        1.0,
        LoopPolicy::Once,
        vec![BoneTrack {
            bone: UPPER_ARM_FRONT,
            rotation: None,
            translation_x: None,
            translation_y: None,
            depth_angle: Some(crate::Channel::from_keys(keys)),
        }],
    )
    .expect("valid depth clip")
}

fn sample_projected(clip: &AnimationClip, t: f32) -> (LocalPose, crate::DepthPose, WorldPose) {
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let mut depth = crate::DepthPose::zeros(def.bone_count());
    crate::sample_and_project(def, clip, t, &mut local, &mut depth).unwrap();
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    (local, depth, world)
}

fn limb_len(
    world: &WorldPose,
    parent: purgatory_skeleton::BoneIndex,
    child: purgatory_skeleton::BoneIndex,
) -> f32 {
    let a = world.get(parent).unwrap().translation;
    let b = world.get(child).unwrap().translation;
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[test]
fn default_depth_reproduces_bind_pose() {
    let clip = depth_clip(linear_keys(&[(0.0, 0.0), (1.0, 0.0)]));
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let mut depth = crate::DepthPose::zeros(def.bone_count());
    crate::sample_and_project(def, &clip, 0.0, &mut local, &mut depth).unwrap();
    let bind = LocalPose::from_bind(def);
    assert_eq!(
        local.get(LOWER_ARM_FRONT).unwrap().translation,
        bind.get(LOWER_ARM_FRONT).unwrap().translation
    );
    assert_eq!(
        local.get(UPPER_ARM_FRONT).unwrap().translation,
        bind.get(UPPER_ARM_FRONT).unwrap().translation
    );
}

#[test]
fn existing_anim_assets_have_no_depth_channel() {
    let clip = a3_idle_clip();
    assert!(
        clip.tracks().iter().all(|t| t.depth_angle.is_none()),
        "A6 assets must remain depth-unkeyed"
    );
    let def = humanoid_v0();
    let mut with_depth = LocalPose::from_bind(def);
    let mut depth = crate::DepthPose::zeros(def.bone_count());
    crate::sample_and_project(def, clip, 0.0, &mut with_depth, &mut depth).unwrap();
    let mut without = LocalPose::from_bind(def);
    sample(clip, 0.0, &mut without).unwrap();
    for i in 0..def.bone_count() {
        let bone = purgatory_skeleton::BoneIndex::from_u8(i as u8);
        assert_eq!(with_depth.get(bone), without.get(bone));
    }
}

#[test]
fn depth_shortens_child_and_keeps_parent_fixed() {
    let clip = depth_clip(linear_keys(&[(0.0, 0.0), (1.0, crate::DEPTH_ANGLE_LIMIT)]));
    let (_, _, world0) = sample_projected(&clip, 0.0);
    let (_, depth1, world1) = sample_projected(&clip, 1.0);
    let parent0 = world0.get(UPPER_ARM_FRONT).unwrap().translation;
    let parent1 = world1.get(UPPER_ARM_FRONT).unwrap().translation;
    assert!((parent0[0] - parent1[0]).abs() < 1e-5);
    assert!((parent0[1] - parent1[1]).abs() < 1e-5);
    let len0 = limb_len(&world0, UPPER_ARM_FRONT, LOWER_ARM_FRONT);
    let len1 = limb_len(&world1, UPPER_ARM_FRONT, LOWER_ARM_FRONT);
    assert!(len1 < len0 * 0.2, "len0={len0} len1={len1}");
    assert!(len1 >= crate::DEPTH_PROJECTION_MIN * 0.20 - 1e-4);
    assert!(depth1.get(UPPER_ARM_FRONT).unwrap() > 1.5);
}

#[test]
fn positive_and_negative_depth_match_length_and_keep_sign() {
    let pos = depth_clip(linear_keys(&[(0.0, 0.8), (1.0, 0.8)]));
    let neg = depth_clip(linear_keys(&[(0.0, -0.8), (1.0, -0.8)]));
    let (_, dpos, wpos) = sample_projected(&pos, 0.0);
    let (_, dneg, wneg) = sample_projected(&neg, 0.0);
    assert!((dpos.get(UPPER_ARM_FRONT).unwrap() - 0.8).abs() < 1e-5);
    assert!((dneg.get(UPPER_ARM_FRONT).unwrap() + 0.8).abs() < 1e-5);
    let lp = limb_len(&wpos, UPPER_ARM_FRONT, LOWER_ARM_FRONT);
    let ln = limb_len(&wneg, UPPER_ARM_FRONT, LOWER_ARM_FRONT);
    assert!((lp - ln).abs() < 1e-5);
    assert_eq!(
        wpos.get(UPPER_ARM_FRONT).unwrap().translation,
        wneg.get(UPPER_ARM_FRONT).unwrap().translation
    );
}

#[test]
fn depth_descendants_follow_hierarchy() {
    let clip = depth_clip(linear_keys(&[(0.0, 0.0), (1.0, 1.2)]));
    let (_, _, world0) = sample_projected(&clip, 0.0);
    let (_, _, world1) = sample_projected(&clip, 1.0);
    let wrist0 = world0
        .get(purgatory_skeleton::HAND_FRONT)
        .unwrap()
        .translation;
    let wrist1 = world1
        .get(purgatory_skeleton::HAND_FRONT)
        .unwrap()
        .translation;
    let elbow0 = world0.get(LOWER_ARM_FRONT).unwrap().translation;
    let elbow1 = world1.get(LOWER_ARM_FRONT).unwrap().translation;
    let d0 = (wrist0[0] - elbow0[0]).hypot(wrist0[1] - elbow0[1]);
    let d1 = (wrist1[0] - elbow1[0]).hypot(wrist1[1] - elbow1[1]);
    assert!(
        (d0 - d1).abs() < 1e-4,
        "forearm rest length must stay; hand follows elbow"
    );
    assert!(elbow1[1] > elbow0[1] - 1e-4, "elbow moves toward shoulder");
}

#[test]
fn depth_does_not_swap_front_back() {
    let clip = depth_clip(linear_keys(&[(0.0, 1.0), (1.0, 1.0)]));
    let (local, _, _) = sample_projected(&clip, 0.0);
    assert!(local.get(UPPER_ARM_FRONT).is_some());
    assert!(local.get(UPPER_ARM_BACK).is_some());
    assert_eq!(UPPER_ARM_FRONT.as_u8(), 4);
    assert_eq!(UPPER_ARM_BACK.as_u8(), 7);
}

#[test]
fn non_finite_and_out_of_range_depth_rejected() {
    let def = humanoid_v0();
    let nan = AnimationClip::try_new(
        def,
        1.0,
        LoopPolicy::Once,
        vec![BoneTrack {
            bone: UPPER_ARM_FRONT,
            rotation: None,
            translation_x: None,
            translation_y: None,
            depth_angle: Some(crate::Channel::from_keys(linear_keys(&[(0.0, f32::NAN)]))),
        }],
    );
    assert!(matches!(nan, Err(ClipError::NonFiniteKeyValue { .. })));
    let wide = AnimationClip::try_new(
        def,
        1.0,
        LoopPolicy::Once,
        vec![BoneTrack {
            bone: UPPER_ARM_FRONT,
            rotation: None,
            translation_x: None,
            translation_y: None,
            depth_angle: Some(crate::Channel::from_keys(linear_keys(&[(
                0.0,
                crate::DEPTH_ANGLE_LIMIT + 0.2,
            )]))),
        }],
    );
    assert!(matches!(wide, Err(ClipError::DepthAngleOutOfRange { .. })));
}

#[test]
fn depth_channel_serializes_and_reloads() {
    let def = humanoid_v0();
    let text = r#"
schema_version 1
duration 1.00
loop Once
track upper_arm_front
depth 0.00 0.0 Linear
depth 1.00 1.2 Linear
endtrack
markers
endmarkers
"#;
    let parsed = crate::parse_animation_asset_v1("depth.anim", text, def).unwrap();
    assert!(parsed.clip.tracks()[0].depth_angle.is_some());
    let serialized = crate::serialize_animation_asset_v1(&parsed);
    let reparsed = crate::parse_animation_asset_v1("depth.anim", &serialized, def).unwrap();
    assert_eq!(parsed, reparsed);
}

#[test]
fn depth_blends_as_authored_channel() {
    let mut from = crate::DepthPose::zeros(humanoid_v0().bone_count());
    let mut to = crate::DepthPose::zeros(humanoid_v0().bone_count());
    from.set(UPPER_ARM_FRONT, 0.0);
    to.set(UPPER_ARM_FRONT, 1.0);
    let mut out = crate::DepthPose::zeros(humanoid_v0().bone_count());
    crate::blend_depth_poses(&from, &to, 0.0, &mut out).unwrap();
    assert!((out.get(UPPER_ARM_FRONT).unwrap() - 0.0).abs() < 1e-6);
    crate::blend_depth_poses(&from, &to, 1.0, &mut out).unwrap();
    assert!((out.get(UPPER_ARM_FRONT).unwrap() - 1.0).abs() < 1e-6);
    crate::blend_depth_poses(&from, &to, 0.5, &mut out).unwrap();
    assert!((out.get(UPPER_ARM_FRONT).unwrap() - 0.5).abs() < 1e-6);
}

#[test]
fn scale_token_is_not_a_depth_alias() {
    let def = humanoid_v0();
    let text = r#"
schema_version 1
duration 1.00
loop Once
track upper_arm_front
scale 0.00 0.5 Linear
endtrack
"#;
    let err = crate::parse_animation_asset_v1("scale.anim", text, def).unwrap_err();
    assert!(err.reason.contains("unknown token"));
}
