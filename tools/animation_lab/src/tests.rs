//! Editor-model tests for A7.0 / A7.1 (not UI pixels).

use std::fs;
use std::path::PathBuf;

use purgatory_animation::{
    AnimationMarker, Interpolation, Keyframe, LoopPolicy, parse_animation_asset_v1,
    serialize_animation_asset_v1,
};
use purgatory_skeleton::{
    HAND_FRONT, HEAD, LOWER_ARM_BACK, LOWER_ARM_FRONT, LocalPose, PELVIS, ROOT, TORSO,
    UPPER_ARM_BACK, UPPER_ARM_FRONT, UPPER_LEG_FRONT, WorldPose, evaluate, humanoid_v0,
    torso_local_corners,
};

use crate::clipboard::{Clipboard, ClipboardKey, copy_keys, copy_pose, paste_keys};
use crate::debug_vis::{
    HAND_PLACEHOLDER_COLOR, HEAD_PLACEHOLDER_COLOR, TORSO_PLACEHOLDER_COLOR,
    UPPER_ARM_BACK_PLACEHOLDER_COLOR, UPPER_ARM_PLACEHOLDER_COLOR, body_panels, is_back_bone,
};
use crate::document::{AnimDocument, ChannelKind, EditableTrack, KeyRef};
use crate::headwear_proof;
use crate::history::EditHistory;
use crate::io::{load_anim_file, save_anim_file, write_validated};
use crate::preview::{apply_direct_rotation, evaluate_preview, fit_preview_camera, mirror_x};
use crate::session::LabSession;
use crate::snap::{JointSnapSettings, SnapSettings, snap_joint_rotation, snap_time};
use crate::timeline::{ROW_H, TimelineStrip, channel_rows, hit_channel_key, hit_marker};

fn head_key(time: f32, value: f32) -> Keyframe {
    Keyframe {
        time,
        value,
        interpolation: Interpolation::Linear,
    }
}

#[test]
fn new_clip_is_valid_empty_document() {
    let doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    let asset = doc.to_asset(humanoid_v0()).unwrap();
    assert!(asset.clip.tracks().is_empty());
    let text = serialize_animation_asset_v1(&asset);
    let reparsed = parse_animation_asset_v1("empty.anim", &text, humanoid_v0()).unwrap();
    assert_eq!(reparsed.clip.duration(), 1.0);
}

#[test]
fn open_a6_round_trip_equivalence() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/shared/animations/dev/a4_fall.anim"
    ));
    let parsed = parse_animation_asset_v1("a4_fall.anim", text, humanoid_v0()).unwrap();
    let doc = AnimDocument::from_asset(&parsed);
    let asset = doc.to_asset(humanoid_v0()).unwrap();
    let serialized = serialize_animation_asset_v1(&asset);
    let reparsed = parse_animation_asset_v1("a4_fall.anim", &serialized, humanoid_v0()).unwrap();
    assert_eq!(parsed, reparsed);
}

#[test]
fn add_edit_delete_move_key() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.1))
        .unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.50, 0.4))
        .unwrap();
    assert_eq!(doc.track(HEAD).unwrap().rotation.len(), 2);

    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.50, 0.8))
        .unwrap();
    assert!((doc.track(HEAD).unwrap().rotation[1].value - 0.8).abs() < 1e-5);

    doc.move_key(HEAD, ChannelKind::Rotation, 1, 0.35).unwrap();
    assert!((doc.track(HEAD).unwrap().rotation[1].time - 0.35).abs() < 1e-5);

    doc.delete_key(HEAD, ChannelKind::Rotation, 0).unwrap();
    assert_eq!(doc.track(HEAD).unwrap().rotation.len(), 1);
}

#[test]
fn direct_manipulation_creates_rotation_key() {
    let mut doc = AnimDocument::empty(0.6, LoopPolicy::Loop).unwrap();
    apply_direct_rotation(&mut doc, UPPER_LEG_FRONT, 0.3, -0.55).unwrap();
    let key = &doc.track(UPPER_LEG_FRONT).unwrap().rotation[0];
    assert!((key.time - 0.30).abs() < 1e-5);
    assert!((key.value - (-0.55)).abs() < 1e-5);
}

#[test]
fn root_and_limb_translation_are_not_authorable() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    let err = doc
        .upsert_key(ROOT, ChannelKind::Rotation, head_key(0.0, 0.1))
        .unwrap_err();
    assert!(err.contains("not authorable") || err.contains("not keyable"));

    let err = doc
        .upsert_key(
            UPPER_LEG_FRONT,
            ChannelKind::TranslationX,
            head_key(0.0, 0.1),
        )
        .unwrap_err();
    assert!(err.contains("not authorable"));

    doc.upsert_key(PELVIS, ChannelKind::TranslationY, head_key(0.0, 0.02))
        .unwrap();
    assert_eq!(doc.track(PELVIS).unwrap().translation_y.len(), 1);
}

#[test]
fn invalid_duration_does_not_corrupt_document() {
    let mut doc = AnimDocument::empty(0.6, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.6, 0.1))
        .unwrap();
    let before = doc.clone();
    assert!(doc.set_duration(0.2).is_err());
    assert_eq!(doc, before);
}

#[test]
fn history_undo_redo_and_jump() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    let mut history = EditHistory::new("New clip", doc.clone());
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.1))
        .unwrap();
    history.push("Add key — head.rot @ 0.00", doc.clone());
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.5, 0.2))
        .unwrap();
    history.push("Add key — head.rot @ 0.50", doc.clone());

    let undone = history.undo().unwrap();
    assert_eq!(undone.track(HEAD).unwrap().rotation.len(), 1);
    let redone = history.redo().unwrap();
    assert_eq!(redone.track(HEAD).unwrap().rotation.len(), 2);
    let jumped = history.jump(0).unwrap();
    assert!(jumped.tracks.is_empty());
}

#[test]
fn edit_after_undo_discards_redo_branch() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    let mut history = EditHistory::new("Open", doc.clone());
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.1))
        .unwrap();
    history.push("A", doc.clone());
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.5, 0.2))
        .unwrap();
    history.push("B", doc.clone());
    history.undo();
    let mut branched = history.current().clone();
    branched
        .upsert_key(HEAD, ChannelKind::Rotation, head_key(0.2, 0.3))
        .unwrap();
    history.push("C", branched);
    assert!(!history.can_redo());
    assert_eq!(history.entries().len(), 3);
    assert_eq!(history.entries()[2].label, "C");
}

#[test]
fn transaction_commit_is_single_history_entry() {
    let mut session = LabSession::open_workspace().expect("workspace root");
    session.begin_transaction("Rotate bone — head");
    let t = session.playhead;
    apply_direct_rotation(session.transaction_working_mut().unwrap(), HEAD, t, 0.5).unwrap();
    apply_direct_rotation(session.transaction_working_mut().unwrap(), HEAD, t, 0.7).unwrap();
    session.commit_transaction().unwrap();
    assert_eq!(session.history.entries().len(), 2);
    assert_eq!(session.history.entries()[1].label, "Rotate bone — head");
    session.undo();
    assert!(session.committed_document().track(HEAD).is_none());
}

#[test]
fn transaction_cancel_restores_pre_drag_document() {
    let mut session = LabSession::open_workspace().expect("workspace root");
    let t = session.playhead;
    session.begin_transaction("Rotate bone — head");
    apply_direct_rotation(session.transaction_working_mut().unwrap(), HEAD, t, 0.9).unwrap();
    assert!(session.document().track(HEAD).is_some());
    session.cancel_transaction();
    assert!(session.committed_document().track(HEAD).is_none());
    assert!(session.document().track(HEAD).is_none());
    assert_eq!(session.history.entries().len(), 1);
}

#[test]
fn key_move_transaction_is_single_history_entry() {
    let mut session = LabSession::open_workspace().expect("workspace root");
    session
        .mutate("Add key", |doc| {
            doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.20, 0.1))
        })
        .unwrap();
    session.begin_transaction("Move key — head.rot @ 0.20");
    session
        .preview_key_time(HEAD, ChannelKind::Rotation, 0, 0.35)
        .unwrap();
    session
        .preview_key_time(HEAD, ChannelKind::Rotation, 0, 0.48)
        .unwrap();
    session.commit_transaction().unwrap();
    assert_eq!(session.history.entries().len(), 3);
    let t = session.committed_document().track(HEAD).unwrap().rotation[0].time;
    assert!((t - 0.48).abs() < 1e-5);
}

#[test]
fn existing_out_of_policy_keys_remain_visible_and_editable() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    let mut root = EditableTrack::new(ROOT);
    root.rotation.push(head_key(0.0, 0.1));
    doc.tracks.push(root);
    assert!(doc.channel_visible(ROOT, ChannelKind::Rotation));
    assert!(!doc.can_add_key(ROOT, ChannelKind::Rotation));
    doc.upsert_key(ROOT, ChannelKind::Rotation, head_key(0.0, 0.25))
        .unwrap();
    assert!((doc.track(ROOT).unwrap().rotation[0].value - 0.25).abs() < 1e-5);
    doc.delete_key(ROOT, ChannelKind::Rotation, 0).unwrap();
    assert!(doc.track(ROOT).is_none());

    let mut limb = EditableTrack::new(UPPER_LEG_FRONT);
    limb.translation_x.push(head_key(0.0, 0.05));
    doc.tracks.push(limb);
    assert!(doc.channel_visible(UPPER_LEG_FRONT, ChannelKind::TranslationX));
    assert!(!doc.can_add_key(UPPER_LEG_FRONT, ChannelKind::TranslationX));
    doc.upsert_key(
        UPPER_LEG_FRONT,
        ChannelKind::TranslationX,
        head_key(0.0, 0.08),
    )
    .unwrap();
    assert!((doc.track(UPPER_LEG_FRONT).unwrap().translation_x[0].value - 0.08).abs() < 1e-5);
}

#[test]
fn new_clip_refuses_existing_filename() {
    let mut session = LabSession::open_workspace().expect("workspace root");
    session.new_clip.name = "a4_fall".to_string();
    session.new_clip.duration = "1.0".to_string();
    let err = session.new_clip().unwrap_err();
    assert!(err.contains("already exists"));
}

#[test]
fn save_as_uses_the_same_validation_path() {
    let dir = std::env::temp_dir().join(format!("purgatory-a7-saveas-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut doc = AnimDocument::empty(0.5, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.2))
        .unwrap();
    let mut session = LabSession::for_test(dir.clone(), dir.clone(), doc);
    session.save_as_name = "variant".to_string();
    session.save_as().unwrap();
    let path = dir.join("variant.anim");
    let loaded = crate::io::load_anim_file(&path).unwrap();
    assert_eq!(session.committed_document(), &loaded);
    parse_animation_asset_v1(
        "variant.anim",
        &fs::read_to_string(&path).unwrap(),
        humanoid_v0(),
    )
    .unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn save_reload_preserves_authored_result() {
    let dir = std::env::temp_dir().join(format!("purgatory-a7-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lab_save.anim");
    let mut doc = AnimDocument::empty(0.8, LoopPolicy::Once).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.1))
        .unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.8, -0.2))
        .unwrap();
    doc.upsert_marker(AnimationMarker {
        time: 0.40,
        name: "peak".to_string(),
        marker_type: "notify".to_string(),
        payload: None,
    })
    .unwrap();
    save_anim_file(&path, &doc).unwrap();
    let loaded = crate::io::load_anim_file(&path).unwrap();
    assert_eq!(doc, loaded);
    let text = fs::read_to_string(&path).unwrap();
    parse_animation_asset_v1("lab_save.anim", &text, humanoid_v0()).unwrap();
    let _ = fs::remove_file(&path);
}

#[test]
fn runtime_parser_accepts_saved_jump_fall_shape() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/shared/animations/dev/a4_jump.anim"
    ));
    let parsed = parse_animation_asset_v1("a4_jump.anim", text, humanoid_v0()).unwrap();
    let mut doc = AnimDocument::from_asset(&parsed);
    apply_direct_rotation(&mut doc, UPPER_LEG_FRONT, 0.3, -0.7).unwrap();
    let asset = doc.to_asset(humanoid_v0()).unwrap();
    let serialized = serialize_animation_asset_v1(&asset);
    parse_animation_asset_v1("a4_jump.anim", &serialized, humanoid_v0()).unwrap();
}

#[test]
fn write_validated_refuses_to_skip_parse() {
    let dir = std::env::temp_dir().join(format!("purgatory-a7-val-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ok.anim");
    let doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    let asset = doc.to_asset(humanoid_v0()).unwrap();
    write_validated(&path, &asset).unwrap();
    assert!(path.is_file());
    let _ = fs::remove_file(&path);
}

#[test]
fn preview_camera_fits_humanoid_and_does_not_change_world() {
    let cam = fit_preview_camera(0.0, 0.0, 840.0, 560.0);
    assert!(
        cam.scale > 280.0,
        "auto-fit should be larger than the old 210 px/unit scale, got {}",
        cam.scale
    );
    let origin = cam.to_canvas([0.0, 0.0]);
    assert!((origin[0] - 420.0).abs() < 2.0);
    let head = cam.to_canvas([0.0, 1.0]);
    assert!(head[1] > 16.0 && head[1] < origin[1]);
    let ground = cam.to_canvas([-0.5, 0.0]);
    assert!((ground[1] - origin[1]).abs() < 1e-3);

    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let before = world.get(HEAD).unwrap().translation;
    let _ = cam.to_canvas(before);
    assert_eq!(world.get(HEAD).unwrap().translation, before);
}

#[test]
fn lab_body_panels_match_phase8_colors_and_draw_back_first() {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let panels = body_panels(def, &world);
    assert_eq!(panels.len(), 15);
    assert_eq!(panels[0].color, UPPER_ARM_BACK_PLACEHOLDER_COLOR);
    let last_back = panels
        .iter()
        .rposition(|p| is_back_bone(p.bone))
        .expect("back panels");
    let first_front_arm = panels
        .iter()
        .position(|p| p.color == UPPER_ARM_PLACEHOLDER_COLOR)
        .expect("front arm");
    assert!(last_back < first_front_arm);
    let head = panels.iter().find(|p| p.bone == HEAD).expect("head panel");
    assert_eq!(head.color, HEAD_PLACEHOLDER_COLOR);
    let head_i = panels.iter().position(|p| p.bone == HEAD).unwrap();
    assert!(head_i < first_front_arm);
}

#[test]
fn lab_headwear_proof_uses_crown_compose_and_sits_in_head_layer() {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let panels = body_panels(def, &world);
    let head_i = panels.iter().position(|p| p.bone == HEAD).unwrap();
    let arm_i = panels
        .iter()
        .position(|p| p.color == UPPER_ARM_PLACEHOLDER_COLOR)
        .unwrap();
    assert!(head_i < arm_i);
    let xf = headwear_proof::compose_crown(&world).unwrap();
    assert_eq!(xf.rotation, world.get(HEAD).unwrap().rotation);
    let corners = headwear_proof::sprite_world_corners(&world).unwrap();
    let crown = xf.translation;
    assert!((headwear_proof::apply_local(xf, [0.0, 0.0])[0] - crown[0]).abs() < 1e-5);
    let _ = corners;
}

fn add_rot(origin: [f32; 2], local: [f32; 2], angle: f32) -> [f32; 2] {
    let (s, c) = angle.sin_cos();
    [
        origin[0] + local[0] * c - local[1] * s,
        origin[1] + local[0] * s + local[1] * c,
    ]
}

#[test]
fn lab_torso_uses_canonical_bone_origin_corners() {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let torso_xf = world.get(TORSO).unwrap();
    let expected: [[f32; 2]; 4] =
        torso_local_corners(1.0).map(|p| add_rot(torso_xf.translation, p, torso_xf.rotation));
    let panels = body_panels(def, &world);
    let torso = panels
        .iter()
        .find(|p| p.bone == TORSO && p.color == TORSO_PLACEHOLDER_COLOR)
        .expect("near torso panel");
    for (got, want) in torso.corners.iter().zip(expected.iter()) {
        assert!((got[0] - want[0]).abs() < 1e-5);
        assert!((got[1] - want[1]).abs() < 1e-5);
    }
    let bottom_y = torso.corners[0][1].min(torso.corners[1][1]);
    let top_y = torso.corners[2][1].max(torso.corners[3][1]);
    assert!(bottom_y < torso_xf.translation[1]);
    assert!(top_y > torso_xf.translation[1]);
}

#[test]
fn lab_torso_stays_on_bone_origin_when_rotated() {
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    local.get_mut(TORSO).unwrap().rotation = 0.4;
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let torso_xf = world.get(TORSO).unwrap();
    let panels = body_panels(def, &world);
    let torso = panels
        .iter()
        .find(|p| p.bone == TORSO && p.color == TORSO_PLACEHOLDER_COLOR)
        .expect("near torso panel");
    for (got, local_c) in torso.corners.iter().zip(torso_local_corners(1.0).iter()) {
        let want = add_rot(torso_xf.translation, *local_c, torso_xf.rotation);
        assert!((got[0] - want[0]).abs() < 1e-5);
        assert!((got[1] - want[1]).abs() < 1e-5);
    }
}

#[test]
fn lab_hand_hangs_along_bone_neg_y_from_wrist() {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    let wrist = world.get(HAND_FRONT).unwrap().translation;
    let panels = body_panels(def, &world);
    let hand = panels
        .iter()
        .find(|p| p.bone == HAND_FRONT && p.color == HAND_PLACEHOLDER_COLOR)
        .expect("front hand");
    let ys: Vec<f32> = hand.corners.iter().map(|c| c[1]).collect();
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!((max_y - wrist[1]).abs() < 1e-4);
    assert!(min_y < wrist[1] - 0.05);
}

fn a5_attack_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/shared/animations/dev/a5_attack.anim")
}

#[test]
fn timeline_strip_maps_zero_to_duration() {
    let strip = TimelineStrip::from_sheet(0.0, 400.0, 0.40);
    assert!((strip.t_of(strip.x0) - 0.0).abs() < 1e-5);
    assert!((strip.t_of(strip.x1) - 0.40).abs() < 1e-5);
    let mid = strip.x_of(0.18);
    assert!((strip.t_of(mid) - 0.18).abs() < 0.011);
    assert!(strip.x_of(0.0) >= strip.x0);
    assert!(strip.x_of(0.40) <= strip.x1);
}

#[test]
fn a5_attack_authored_keys_are_timeline_rows() {
    let doc = load_anim_file(&a5_attack_path()).expect("a5_attack.anim");
    let rows = channel_rows(&doc, None, ChannelKind::Rotation);
    assert_eq!(rows.len(), 5);
    assert!(
        rows.iter()
            .any(|r| r.bone == UPPER_ARM_FRONT && r.kind == ChannelKind::Rotation)
    );
    assert!(
        rows.iter()
            .any(|r| r.bone == TORSO && r.kind == ChannelKind::Rotation)
    );
    assert!(
        rows.iter()
            .any(|r| r.bone == HEAD && r.kind == ChannelKind::Rotation)
    );
    assert!(
        rows.iter()
            .any(|r| r.bone == UPPER_ARM_BACK && r.kind == ChannelKind::Rotation)
    );
    assert!(
        rows.iter()
            .any(|r| r.bone == LOWER_ARM_BACK && r.kind == ChannelKind::Rotation)
    );
    let selected = channel_rows(&doc, Some(HEAD), ChannelKind::Rotation);
    assert_eq!(
        selected.len(),
        5,
        "selecting a bone must not hide other tracks"
    );
}

#[test]
fn timeline_hit_selects_authored_key_at_time() {
    let doc = load_anim_file(&a5_attack_path()).expect("a5_attack.anim");
    let rows = channel_rows(&doc, None, ChannelKind::Rotation);
    let strip = TimelineStrip::from_sheet(0.0, 500.0, doc.duration);
    let front_row = rows
        .iter()
        .position(|r| r.bone == UPPER_ARM_FRONT && r.kind == ChannelKind::Rotation)
        .unwrap();
    let body_top = 40.0;
    let y = body_top + front_row as f32 * ROW_H + ROW_H * 0.5;
    let x = strip.x_of(0.18);
    let hit = hit_channel_key(strip, &rows, body_top, [x, y], &doc).expect("key at 0.18");
    assert_eq!(hit.0, UPPER_ARM_FRONT);
    assert_eq!(hit.1, ChannelKind::Rotation);
}

#[test]
fn reserved_marker_row_hits_existing_marker() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.markers.push(AnimationMarker {
        time: 0.25,
        name: "hit".into(),
        marker_type: "notify".into(),
        payload: None,
    });
    let strip = TimelineStrip::from_sheet(0.0, 400.0, 1.0);
    let y = 10.0 + 11.0;
    let hit = hit_marker(strip, y, [strip.x_of(0.25), y], &doc);
    assert_eq!(hit, Some(0));
    assert!(hit_marker(strip, y, [strip.x_of(0.80), y], &doc).is_none());
}

#[test]
fn duplicate_key_move_does_not_corrupt_committed_document() {
    let mut session = LabSession::open_workspace().expect("workspace root");
    session
        .mutate("seed", |doc| {
            doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.1))?;
            doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.30, 0.2))?;
            Ok(())
        })
        .unwrap();
    let before = session.committed_document().clone();
    session.begin_transaction("Move key");
    let err = session
        .preview_key_time(HEAD, ChannelKind::Rotation, 0, 0.30)
        .expect_err("duplicate time");
    assert!(err.contains("duplicate"));
    session.cancel_transaction();
    assert_eq!(session.committed_document(), &before);
    assert_eq!(session.document().track(HEAD).unwrap().rotation.len(), 2);
}

#[test]
fn invalid_key_draft_does_not_commit() {
    let mut session = LabSession::open_workspace().expect("workspace root");
    session
        .mutate("seed", |doc| {
            doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.1))
        })
        .unwrap();
    session.select_bone(HEAD);
    session.selected_channel = ChannelKind::Rotation;
    session.select_key(0);
    session.key_time_draft = "not-a-time".into();
    session.key_value_draft = "0.2".into();
    let before = session.committed_document().clone();
    assert!(session.commit_key_drafts().is_err());
    assert_eq!(session.committed_document(), &before);
}

fn test_session(doc: AnimDocument) -> LabSession {
    LabSession::for_test(PathBuf::from("test-root"), PathBuf::from("test-anim"), doc)
}

#[test]
fn copy_paste_preserves_relative_timing() {
    let mut doc = AnimDocument::empty(2.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.1))
        .unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.40, 0.4))
        .unwrap();
    let refs = vec![
        KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.10),
        KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.40),
    ];
    let clip = copy_keys(&doc, &refs);
    assert!((clip.keys[0].rel_time - 0.0).abs() < 1e-5);
    assert!((clip.keys[1].rel_time - 0.30).abs() < 1e-5);
    let pasted = paste_keys(&doc, &clip, 1.00).unwrap();
    assert!((pasted[0].2.time - 1.00).abs() < 1e-5);
    assert!((pasted[1].2.time - 1.30).abs() < 1e-5);
    assert!((pasted[0].2.value - 0.1).abs() < 1e-5);
    assert!((pasted[1].2.value - 0.4).abs() < 1e-5);
    assert_eq!(pasted[0].2.interpolation, Interpolation::Linear);
    assert_eq!(pasted[0].1, ChannelKind::Rotation);
    assert_eq!(pasted[0].0, HEAD);
}

#[test]
fn multi_channel_paste_and_invalid_is_atomic() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.00, 0.1))
        .unwrap();
    doc.upsert_key(TORSO, ChannelKind::Rotation, head_key(0.20, 0.2))
        .unwrap();
    let clip = copy_keys(
        &doc,
        &[
            KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.00),
            KeyRef::from_key(TORSO, ChannelKind::Rotation, 0.20),
        ],
    );
    doc.upsert_keys(&paste_keys(&doc, &clip, 0.50).unwrap())
        .unwrap();
    assert_eq!(doc.track(HEAD).unwrap().rotation.len(), 2);
    assert_eq!(doc.track(TORSO).unwrap().rotation.len(), 2);

    let collapse = crate::clipboard::Clipboard {
        keys: vec![
            crate::clipboard::ClipboardKey {
                bone: HEAD,
                kind: ChannelKind::Rotation,
                rel_time: 0.00,
                value: 0.1,
                interpolation: Interpolation::Linear,
            },
            crate::clipboard::ClipboardKey {
                bone: HEAD,
                kind: ChannelKind::Rotation,
                rel_time: 0.01,
                value: 0.2,
                interpolation: Interpolation::Linear,
            },
        ],
    };
    assert!(paste_keys(&doc, &collapse, 0.995).is_err());
    let root_clip = crate::clipboard::Clipboard {
        keys: vec![crate::clipboard::ClipboardKey {
            bone: ROOT,
            kind: ChannelKind::Rotation,
            rel_time: 0.0,
            value: 0.0,
            interpolation: Interpolation::Linear,
        }],
    };
    assert!(paste_keys(&doc, &root_clip, 0.0).is_err());
    assert_eq!(doc.track(HEAD).unwrap().rotation.len(), 2);
}

#[test]
fn batch_move_delete_undo_redo() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.1))
        .unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.30, 0.2))
        .unwrap();
    let mut session = test_session(doc);
    session.mutate("seed already in doc", |_| Ok(())).unwrap();
    let refs = [
        KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.10),
        KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.30),
    ];
    session
        .mutate("Move 2 keys", |d| {
            d.move_keys(&[(refs[0], 0.20), (refs[1], 0.40)]).map(|_| ())
        })
        .unwrap();
    let times: Vec<_> = session
        .document()
        .track(HEAD)
        .unwrap()
        .rotation
        .iter()
        .map(|k| k.time)
        .collect();
    assert_eq!(times, vec![0.20, 0.40]);
    session
        .mutate("Delete 2", |d| {
            d.delete_keys(&[
                KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.20),
                KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.40),
            ])
        })
        .unwrap();
    assert!(session.document().track(HEAD).is_none());
    session.undo();
    assert_eq!(session.document().track(HEAD).unwrap().rotation.len(), 2);
    session.redo();
    assert!(session.document().track(HEAD).is_none());
    session.undo();
    session
        .mutate("edit after undo", |d| {
            d.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.05, 0.3))
        })
        .unwrap();
    session.redo();
    assert!(
        session
            .document()
            .track(HEAD)
            .unwrap()
            .rotation
            .iter()
            .any(|k| (k.time - 0.05).abs() < 1e-5)
    );
}

#[test]
fn snap_grid_key_marker_disabled_and_duplicate_safe() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.20, 0.1))
        .unwrap();
    doc.upsert_marker(AnimationMarker {
        time: 0.35,
        name: "hit".into(),
        marker_type: "notify".into(),
        payload: None,
    })
    .unwrap();
    let mut settings = SnapSettings {
        enabled: false,
        ..SnapSettings::default()
    };
    let raw = snap_time(0.24, 1.0, settings, &doc, &[], None);
    assert!((raw - 0.24).abs() < 1e-5);

    settings.enabled = true;
    settings.keys = true;
    settings.markers = false;
    settings.grid = false;
    let to_key = snap_time(0.22, 1.0, settings, &doc, &[], None);
    assert!((to_key - 0.20).abs() < 1e-5);

    settings.keys = false;
    settings.markers = true;
    let to_mark = snap_time(0.36, 1.0, settings, &doc, &[], None);
    assert!((to_mark - 0.35).abs() < 1e-5);

    settings.markers = false;
    settings.grid = true;
    settings.grid_step = 0.10;
    let to_grid = snap_time(0.14, 1.0, settings, &doc, &[], None);
    assert!((to_grid - 0.10).abs() < 1e-5);

    settings.keys = true;
    settings.markers = true;
    settings.grid = true;
    settings.grid_step = 0.10;
    // key 0.20 is closer/higher precedence than grid 0.20 anyway; near 0.34 marker wins over grid 0.30
    let prec = snap_time(0.34, 1.0, settings, &doc, &[], None);
    assert!((prec - 0.35).abs() < 1e-5);

    let dest = KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.00);
    let skipped = snap_time(0.21, 1.0, settings, &doc, &[], Some(dest));
    assert!(
        (skipped - 0.20).abs() > 1e-4,
        "must not snap onto an existing key on the destination channel"
    );
}

#[test]
fn copy_pose_paste_pose_and_depth_key() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(UPPER_ARM_FRONT, ChannelKind::DepthAngle, head_key(0.0, 0.8))
        .unwrap();
    let pose = copy_pose(&doc, 0.0);
    assert!(
        pose.keys
            .iter()
            .any(|k| k.bone == UPPER_ARM_FRONT && k.kind == ChannelKind::DepthAngle)
    );
    let mut session = test_session(doc);
    session.playhead = 0.50;
    session.clipboard = pose;
    session.paste_pose().unwrap();
    assert!(
        session
            .document()
            .track(UPPER_ARM_FRONT)
            .unwrap()
            .depth_angle
            .iter()
            .any(|k| (k.time - 0.50).abs() < 1e-5 && (k.value - 0.8).abs() < 1e-5)
    );
}

#[test]
fn mirror_and_transition_do_not_dirty() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.2))
        .unwrap();
    let mut session = test_session(doc);
    session.saved_document = Some(session.committed_document().clone());
    assert!(!session.is_dirty());
    session.mirror_left = true;
    let before = session.committed_document().clone();
    let pose = session.preview_pose().unwrap();
    let root_x = pose.world.get(ROOT).unwrap().translation[0];
    let head = pose.world.get(HEAD).unwrap().translation;
    let mirrored = mirror_x(head, root_x);
    assert!((mirrored[0] - (2.0 * root_x - head[0])).abs() < 1e-5);
    assert!(!is_back_bone(UPPER_ARM_FRONT));
    assert!(is_back_bone(UPPER_ARM_BACK));
    session.transition_enabled = true;
    session.transition_alpha = 0.0;
    assert!(!session.is_dirty());
    assert_eq!(session.committed_document(), &before);
    let a = evaluate_preview(session.document(), 0.0, None, 0.0).unwrap();
    let b_doc = AnimDocument::empty(1.0, LoopPolicy::Once).unwrap();
    let blended0 = evaluate_preview(session.document(), 0.0, Some(&b_doc), 0.0).unwrap();
    assert_eq!(
        a.world.get(HEAD).unwrap().rotation,
        blended0.world.get(HEAD).unwrap().rotation
    );
    let blended1 = evaluate_preview(session.document(), 0.0, Some(&b_doc), 1.0).unwrap();
    assert_eq!(
        blended1.world.get(HEAD).unwrap().rotation,
        evaluate_preview(&b_doc, 0.0, None, 0.0)
            .unwrap()
            .world
            .get(HEAD)
            .unwrap()
            .rotation
    );
}

#[test]
fn lab_depth_matches_shared_runtime() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(UPPER_ARM_FRONT, ChannelKind::DepthAngle, head_key(0.0, 0.9))
        .unwrap();
    let lab = crate::preview::evaluate_document(&doc, 0.0).unwrap();
    let def = humanoid_v0();
    let clip = doc.to_clip(def).unwrap();
    let mut local = LocalPose::from_bind(def);
    let mut depth = purgatory_animation::DepthPose::zeros(def.bone_count());
    purgatory_animation::sample_and_project(def, &clip, 0.0, &mut local, &mut depth).unwrap();
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).unwrap();
    assert_eq!(
        lab.world.get(LOWER_ARM_FRONT).unwrap().translation,
        world.get(LOWER_ARM_FRONT).unwrap().translation
    );
    assert_eq!(lab.depth.get(UPPER_ARM_FRONT), depth.get(UPPER_ARM_FRONT));
}

#[test]
fn paste_is_one_history_entry_and_invalid_leaves_document() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.1))
        .unwrap();
    let mut session = test_session(doc);
    session.saved_document = Some(session.committed_document().clone());
    session.replace_selection(
        vec![KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.10)],
        None,
    );
    session.copy_selection().unwrap();
    assert!(!session.is_dirty());
    session.playhead = 0.50;
    session.paste_clipboard().unwrap();
    assert_eq!(session.history.entries().len(), 2);
    assert!(
        session.history.entries()[1]
            .label
            .starts_with("Paste 1 key")
    );
    assert_eq!(session.document().track(HEAD).unwrap().rotation.len(), 2);

    let before = session.committed_document().clone();
    session.clipboard = Clipboard {
        keys: vec![ClipboardKey {
            bone: ROOT,
            kind: ChannelKind::Rotation,
            rel_time: 0.0,
            value: 0.0,
            interpolation: Interpolation::Linear,
        }],
    };
    assert!(session.paste_clipboard().is_err());
    assert_eq!(session.committed_document(), &before);
    assert_eq!(session.history.entries().len(), 2);
}

#[test]
fn copy_snap_selection_do_not_dirty() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.1))
        .unwrap();
    let mut session = test_session(doc);
    session.saved_document = Some(session.committed_document().clone());
    session.replace_selection(
        vec![KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.10)],
        None,
    );
    session.copy_selection().unwrap();
    session.snap.enabled = true;
    session.snap.grid_step = 0.05;
    session.set_playhead(0.33);
    assert!(!session.is_dirty());
    assert_eq!(session.history.entries().len(), 1);
}

#[test]
fn invalid_transition_b_fails_cleanly() {
    let mut session = test_session(AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap());
    session.saved_document = Some(session.committed_document().clone());
    assert!(
        session
            .set_transition_clip(PathBuf::from("definitely-missing-a71.anim"))
            .is_err()
    );
    assert!(session.transition_b.is_none());
    assert!(!session.is_dirty());
}

#[test]
fn transition_preview_blends_depth_without_serializing() {
    let mut a = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    a.upsert_key(UPPER_ARM_FRONT, ChannelKind::DepthAngle, head_key(0.0, 0.0))
        .unwrap();
    let mut b = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    b.upsert_key(UPPER_ARM_FRONT, ChannelKind::DepthAngle, head_key(0.0, 0.8))
        .unwrap();
    let p0 = evaluate_preview(&a, 0.0, Some(&b), 0.0).unwrap();
    let p1 = evaluate_preview(&a, 0.0, Some(&b), 1.0).unwrap();
    let p05 = evaluate_preview(&a, 0.0, Some(&b), 0.5).unwrap();
    assert!((p0.depth.get(UPPER_ARM_FRONT).unwrap() - 0.0).abs() < 1e-5);
    assert!((p1.depth.get(UPPER_ARM_FRONT).unwrap() - 0.8).abs() < 1e-5);
    assert!((p05.depth.get(UPPER_ARM_FRONT).unwrap() - 0.4).abs() < 1e-5);
    let serialized_a = serialize_animation_asset_v1(&a.to_asset(humanoid_v0()).unwrap());
    assert!(
        !serialized_a.contains("0.40"),
        "preview blend must not rewrite clip A"
    );
}

#[test]
fn paste_preserves_value_for_rot_tx_ty_depth() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.10, 0.55))
        .unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.40, 1.20))
        .unwrap();
    doc.upsert_key(PELVIS, ChannelKind::TranslationX, head_key(0.10, 0.03))
        .unwrap();
    doc.upsert_key(PELVIS, ChannelKind::TranslationY, head_key(0.10, -0.02))
        .unwrap();
    doc.upsert_key(
        UPPER_ARM_FRONT,
        ChannelKind::DepthAngle,
        head_key(0.10, 0.70),
    )
    .unwrap();
    let mut session = test_session(doc);
    session.replace_selection(
        vec![
            KeyRef::from_key(HEAD, ChannelKind::Rotation, 0.10),
            KeyRef::from_key(PELVIS, ChannelKind::TranslationX, 0.10),
            KeyRef::from_key(PELVIS, ChannelKind::TranslationY, 0.10),
            KeyRef::from_key(UPPER_ARM_FRONT, ChannelKind::DepthAngle, 0.10),
        ],
        None,
    );
    session.copy_selection().unwrap();
    session.playhead = 0.80;
    session.paste_clipboard().unwrap();

    let head_rot = session.document().track(HEAD).unwrap().rotation.clone();
    let pasted_rot = head_rot.iter().find(|k| (k.time - 0.80).abs() < 1e-5);
    assert!((pasted_rot.unwrap().value - 0.55).abs() < 1e-5);
    assert!(
        (head_rot
            .iter()
            .find(|k| (k.time - 0.40).abs() < 1e-5)
            .unwrap()
            .value
            - 1.20)
            .abs()
            < 1e-5
    );

    let pelvis = session.document().track(PELVIS).unwrap();
    assert!(
        (pelvis
            .translation_x
            .iter()
            .find(|k| (k.time - 0.80).abs() < 1e-5)
            .unwrap()
            .value
            - 0.03)
            .abs()
            < 1e-5
    );
    assert!(
        (pelvis
            .translation_y
            .iter()
            .find(|k| (k.time - 0.80).abs() < 1e-5)
            .unwrap()
            .value
            + 0.02)
            .abs()
            < 1e-5
    );
    assert!(
        (session
            .document()
            .track(UPPER_ARM_FRONT)
            .unwrap()
            .depth_angle
            .iter()
            .find(|k| (k.time - 0.80).abs() < 1e-5)
            .unwrap()
            .value
            - 0.70)
            .abs()
            < 1e-5
    );
}

#[test]
fn set_current_pose_as_clip_start_lets_later_keys_interpolate_from_bind() {
    let mut session = test_session(AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap());
    session.playhead = 0.0;
    session.set_current_pose_as_clip_start().unwrap();
    session
        .mutate("later arm", |d| {
            d.upsert_key(UPPER_ARM_FRONT, ChannelKind::Rotation, head_key(0.40, 0.80))
        })
        .unwrap();
    let bind = humanoid_v0().bind_local(UPPER_ARM_FRONT).unwrap().rotation;
    let at0 = evaluate_preview(session.document(), 0.0, None, 0.0).unwrap();
    let at40 = evaluate_preview(session.document(), 0.40, None, 0.0).unwrap();
    assert!((at0.local.get(UPPER_ARM_FRONT).unwrap().rotation - bind).abs() < 1e-4);
    assert!((at40.local.get(UPPER_ARM_FRONT).unwrap().rotation - 0.80).abs() < 1e-4);
    session.undo();
    session.undo();
    session
        .mutate("arm only at 0.40", |d| {
            d.upsert_key(UPPER_ARM_FRONT, ChannelKind::Rotation, head_key(0.40, 0.80))
        })
        .unwrap();
    let held = evaluate_preview(session.document(), 0.0, None, 0.0).unwrap();
    assert!((held.local.get(UPPER_ARM_FRONT).unwrap().rotation - 0.80).abs() < 1e-4);
}

#[test]
fn joint_snap_is_editor_state_and_snaps_rotation() {
    let mut session = test_session(AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap());
    session.saved_document = Some(session.committed_document().clone());
    session.joint_snap.enabled = true;
    session.joint_snap.step_deg = 15.0;
    assert!(!session.is_dirty());
    let raw = 11.0f32.to_radians();
    let snapped = snap_joint_rotation(raw, session.joint_snap);
    assert!((snapped.to_degrees() - 15.0).abs() < 1e-3);
    let off = snap_joint_rotation(raw, JointSnapSettings::default());
    assert!((off - raw).abs() < 1e-6);
}

#[test]
fn transition_advances_without_clip_playback_and_requires_b() {
    let mut a = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    a.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.0))
        .unwrap();
    let mut session = test_session(a);
    session.saved_document = Some(session.committed_document().clone());
    session.transition_enabled = true;
    assert!(session.preview_pose().is_err());
    let mut b = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    b.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.0, 0.9))
        .unwrap();
    session.transition_b = Some((PathBuf::from("b.anim"), b));
    session.transition_playing = true;
    session.playing = false;
    session.transition_alpha = 0.0;
    session.advance_transition(0.10);
    assert!(session.transition_alpha > 0.0);
    assert!(session.transition_alpha < 1.0);
    session.transition_alpha = 1.0;
    let posed = session.preview_pose().unwrap();
    assert!((posed.local.get(HEAD).unwrap().rotation - 0.9).abs() < 1e-4);
    assert!(!session.is_dirty());
}

#[test]
fn delete_keys_at_playhead_is_one_history_entry() {
    let mut doc = AnimDocument::empty(1.0, LoopPolicy::Loop).unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.20, 0.1))
        .unwrap();
    doc.upsert_key(TORSO, ChannelKind::Rotation, head_key(0.20, 0.2))
        .unwrap();
    doc.upsert_key(HEAD, ChannelKind::Rotation, head_key(0.50, 0.3))
        .unwrap();
    let mut session = test_session(doc);
    session.playhead = 0.20;
    session.select_keys_at_playhead();
    assert_eq!(session.selected_keys.len(), 2);
    session.delete_keys_at_playhead().unwrap();
    assert_eq!(session.document().track(HEAD).unwrap().rotation.len(), 1);
    assert!(session.document().track(TORSO).is_none());
    session.undo();
    assert_eq!(session.document().track(HEAD).unwrap().rotation.len(), 2);
    assert_eq!(session.document().track(TORSO).unwrap().rotation.len(), 1);
}
