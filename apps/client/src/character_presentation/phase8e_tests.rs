//! Headless Phase 8E proofs. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{
    AnchorPoint, BoneTarget, ContentRegistry, CorrectionOffset, CoverageMode, LoadMode,
    default_content_root, load_registry,
};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;
use purgatory_skeleton::{BoneTransform, HAND_FRONT, TORSO};

use super::adapters::{from_local, from_remote};
use super::anchors::anchor_local;
use super::bone_map::{BoneTargetMap, hide_bit, hide_contains};
use super::compose::{AUTHORING_CANVAS_PX, compose_attachment, correction_local};
use super::debug_visual::debug_shape;
use super::resolve::{
    BoundAttachment, MissingPresentationReason, resolve_equipment, union_hidden_base,
};
use super::skeleton_input::prepared_from_state;
use super::state::{CharacterPresentationState, EquipmentView, Facing};
use super::{CharacterPresentationSet, LocalMotion, PresentationEntityKey, RemoteMotion};

fn pack() -> ContentRegistry {
    load_registry(&default_content_root(), LoadMode::Shared).expect("shared content")
}

fn cid(authored: &str) -> ContentId {
    ContentId::from_authored(authored).expect("authored id")
}

fn present_one(slot: EquipmentSlot, id: ContentId) -> EquipmentView {
    let mut slots = [None; EquipmentSlot::COUNT];
    slots[slot.index()] = Some(id);
    EquipmentView::present(slots)
}

fn idle(equipment: EquipmentView) -> CharacterPresentationState {
    from_local(
        LocalMotion {
            pose: [1.0, 2.0],
            velocity: [0.0, 0.0],
            grounded: true,
            equipment,
        },
        Facing::Right,
    )
}

fn bound_stub(coverage: CoverageMode, hide: &[BoneTarget]) -> BoundAttachment {
    BoundAttachment {
        slot: EquipmentSlot::Bodywear,
        content_id: ContentId::from_token(1),
        attachment_id: "stub".into(),
        visual_key_side: "stub.side".into(),
        visual_key_back: None,
        bone: TORSO,
        bone_target: BoneTarget::Torso,
        anchor: AnchorPoint::Chest,
        coverage,
        hide_base: hide.iter().copied().fold(0, |acc, t| acc | hide_bit(t)),
        correction: BoneTransform::IDENTITY,
    }
}

#[test]
fn protocol_version_is_current() {
    assert_eq!(PROTOCOL_VERSION, 20);
}

#[test]
fn unadorned_resolves_zero_attachments() {
    let registry = pack();
    let id = cid("equipment.debug.unadorned");
    let out = resolve_equipment(
        present_one(EquipmentSlot::Headwear, id),
        &registry,
        BoneTargetMap::bind_humanoid_v0().unwrap(),
    );
    assert!(out.bound.is_empty());
    assert!(out.missing.is_empty());
    assert_eq!(out.hidden_base, 0);
}

#[test]
fn cloth_cap_and_practice_sword_resolve_fixture_counts() {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    let cap = resolve_equipment(
        present_one(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap")),
        &registry,
        map,
    );
    assert_eq!(cap.bound.len(), 1);
    assert_eq!(cap.bound[0].anchor, AnchorPoint::Crown);
    assert_eq!(cap.bound[0].coverage, CoverageMode::Overlay);
    assert_eq!(
        cap.bound[0].visual_key_side,
        "equipment.debug.cloth_cap.side"
    );
    assert_eq!(
        cap.bound[0].visual_key_back.as_deref(),
        Some("equipment.debug.cloth_cap.back")
    );
    assert_eq!(cap.hidden_base, 0);

    let sword = resolve_equipment(
        present_one(EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
        &registry,
        map,
    );
    assert_eq!(sword.bound.len(), 1);
    assert_eq!(sword.bound[0].anchor, AnchorPoint::GripFront);
    assert_eq!(sword.bound[0].bone, HAND_FRONT);
}

#[test]
fn plate_cuirass_unions_replace_base_hides() {
    let registry = pack();
    let out = resolve_equipment(
        present_one(
            EquipmentSlot::Bodywear,
            cid("equipment.debug.plate_cuirass"),
        ),
        &registry,
        BoneTargetMap::bind_humanoid_v0().unwrap(),
    );
    assert_eq!(out.bound.len(), 1);
    assert_eq!(out.bound[0].coverage, CoverageMode::ReplaceBase);
    assert!(hide_contains(out.hidden_base, BoneTarget::Torso));
    assert!(hide_contains(out.hidden_base, BoneTarget::UpperArmFront));
    assert!(hide_contains(out.hidden_base, BoneTarget::UpperArmBack));
    assert!(!hide_contains(out.hidden_base, BoneTarget::Head));
}

#[test]
fn overlapping_replace_base_hides_union_once() {
    let a = bound_stub(
        CoverageMode::ReplaceBase,
        &[BoneTarget::Torso, BoneTarget::UpperArmFront],
    );
    let b = bound_stub(
        CoverageMode::ReplaceBase,
        &[BoneTarget::Torso, BoneTarget::UpperArmBack],
    );
    let overlay = bound_stub(CoverageMode::Overlay, &[BoneTarget::Head]);
    let mask = union_hidden_base(&[a, b, overlay]);
    assert!(hide_contains(mask, BoneTarget::Torso));
    assert!(hide_contains(mask, BoneTarget::UpperArmFront));
    assert!(hide_contains(mask, BoneTarget::UpperArmBack));
    assert!(
        !hide_contains(mask, BoneTarget::Head),
        "Overlay hide bits must not enter the union"
    );
}

#[test]
fn missing_content_skips_slot_without_substitute() {
    let registry = pack();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();
    let unknown = resolve_equipment(
        present_one(EquipmentSlot::Weapon, ContentId::from_token(999_001)),
        &registry,
        map,
    );
    assert!(unknown.bound.is_empty());
    assert_eq!(unknown.missing.len(), 1);
    assert_eq!(
        unknown.missing[0].reason,
        MissingPresentationReason::UnknownContent
    );

    let gameplay_only = {
        let mut slots = [None; EquipmentSlot::COUNT];
        slots[EquipmentSlot::Weapon.index()] = Some(cid("equipment.debug.practice_sword"));
        slots[EquipmentSlot::Headwear.index()] = Some(ContentId::from_token(42));
        EquipmentView::present(slots)
    };
    let mixed = resolve_equipment(gameplay_only, &registry, map);
    assert_eq!(mixed.bound.len(), 1);
    assert_eq!(mixed.missing.len(), 1);
    assert_eq!(mixed.missing[0].slot, EquipmentSlot::Headwear);
}

#[test]
fn correction_converts_px_and_degrees_without_reclamp() {
    let c = CorrectionOffset {
        x: 2.0,
        y: -1.0,
        rotation: 5.0,
    };
    let t = correction_local(c);
    assert!((t.translation[0] - 2.0 / AUTHORING_CANVAS_PX).abs() < 1e-6);
    assert!((t.translation[1] + 1.0 / AUTHORING_CANVAS_PX).abs() < 1e-6);
    assert!((t.rotation - 5.0_f32.to_radians()).abs() < 1e-6);
}

#[test]
fn compose_is_bone_then_anchor_then_correction() {
    let registry = pack();
    let prepared = prepared_from_state(idle(EquipmentView::Absent)).expect("bind");
    let world = prepared.world();
    let map = BoneTargetMap::bind_humanoid_v0().unwrap();

    let origin = resolve_equipment(
        present_one(EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
        &registry,
        map,
    );
    let front = origin
        .bound
        .iter()
        .find(|b| b.bone_target == BoneTarget::HandFront)
        .expect("front glove");
    let composed = compose_attachment(front, world).expect("hand");
    let bone = world.get(front.bone).unwrap();
    assert!((composed.translation[0] - bone.translation[0]).abs() < 1e-5);
    assert!((composed.translation[1] - bone.translation[1]).abs() < 1e-5);

    let boots = resolve_equipment(
        present_one(EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
        &registry,
        map,
    );
    let front_boot = boots
        .bound
        .iter()
        .find(|b| b.bone_target == BoneTarget::FootFront)
        .expect("front boot");
    let composed_boot = compose_attachment(front_boot, world).expect("foot");
    let expected = world
        .get(front_boot.bone)
        .unwrap()
        .compose(anchor_local(AnchorPoint::FootFront))
        .compose(front_boot.correction);
    assert!((composed_boot.translation[0] - expected.translation[0]).abs() < 1e-5);
    assert!((composed_boot.rotation - expected.rotation).abs() < 1e-5);
    let origin_boot = world.get(front_boot.bone).unwrap();
    assert!(
        (composed_boot.translation[0] - origin_boot.translation[0]).abs() > 1e-4
            || (composed_boot.rotation - origin_boot.rotation).abs() > 1e-4,
        "nonzero correction must move the composed transform"
    );
}

#[test]
fn local_and_remote_share_one_compose_path() {
    let registry = pack();
    let sword = cid("equipment.debug.practice_sword");
    let equipment = present_one(EquipmentSlot::Weapon, sword);
    let local = from_local(
        LocalMotion {
            pose: [0.0, 1.0],
            velocity: [0.0, 0.0],
            grounded: true,
            equipment,
        },
        Facing::Right,
    );
    let remote = from_remote(
        RemoteMotion {
            pose: [4.0, 1.0],
            velocity: [0.0, 0.0],
            equipment,
        },
        Facing::Right,
    );
    let mut set = CharacterPresentationSet::new();
    let a = PresentationEntityKey::new(1, 1);
    let b = PresentationEntityKey::new(2, 1);
    set.sync([(a, local), (b, remote)], &registry, 0.0);
    let la = set.get(a).unwrap();
    let lb = set.get(b).unwrap();
    assert_eq!(la.bound().len(), lb.bound().len());
    assert_eq!(la.bound().len(), 1);
    let ca = compose_attachment(&la.bound()[0], la.prepared().world).unwrap();
    let cb = compose_attachment(&lb.bound()[0], lb.prepared().world).unwrap();
    let ba = la.prepared().world.get(la.bound()[0].bone).unwrap();
    let bb = lb.prepared().world.get(lb.bound()[0].bone).unwrap();
    assert!(
        ((ca.translation[0] - ba.translation[0]) - (cb.translation[0] - bb.translation[0])).abs()
            < 1e-5
    );
    assert!((ca.rotation - ba.rotation - (cb.rotation - bb.rotation)).abs() < 1e-5);
}

#[test]
fn stable_sync_does_not_reresolve_or_reallocate_bound() {
    let registry = pack();
    let cap = cid("equipment.debug.cloth_cap");
    let key = PresentationEntityKey::new(7, 1);
    let state = idle(present_one(EquipmentSlot::Headwear, cap));
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, state)], &registry, 0.0);
    assert_eq!(set.resolve_count(), 1);
    let ptr = set.get(key).unwrap().bound().as_ptr();
    let cap_len = set.get(key).unwrap().bound().len();
    set.sync([(key, state)], &registry, 0.0);
    set.sync([(key, state)], &registry, 0.0);
    assert_eq!(set.resolve_count(), 1);
    assert_eq!(set.get(key).unwrap().bound().as_ptr(), ptr);
    assert_eq!(set.get(key).unwrap().bound().len(), cap_len);

    let sword = idle(present_one(
        EquipmentSlot::Weapon,
        cid("equipment.debug.practice_sword"),
    ));
    set.sync([(key, sword)], &registry, 0.0);
    assert_eq!(set.resolve_count(), 2);
}

#[test]
fn debug_shape_uses_semantics_not_visual_key_substring() {
    let blade = debug_shape(
        EquipmentSlot::Weapon,
        AnchorPoint::GripFront,
        CoverageMode::Overlay,
        BoneTarget::HandFront,
    );
    assert_eq!(blade, super::debug_visual::DebugShape::Blade);
    let not_named_blade = debug_shape(
        EquipmentSlot::Pants,
        AnchorPoint::BoneOrigin,
        CoverageMode::Overlay,
        BoneTarget::UpperLegFront,
    );
    assert_eq!(not_named_blade, super::debug_visual::DebugShape::LimbRect);
    let cap = debug_shape(
        EquipmentSlot::Headwear,
        AnchorPoint::Crown,
        CoverageMode::Overlay,
        BoneTarget::Head,
    );
    assert_eq!(cap, super::debug_visual::DebugShape::Cap);
}

#[test]
fn leave_drops_bound_cache() {
    let registry = pack();
    let key = PresentationEntityKey::new(3, 1);
    let other = PresentationEntityKey::new(4, 1);
    let a = idle(present_one(
        EquipmentSlot::Headwear,
        cid("equipment.debug.cloth_cap"),
    ));
    let b = from_remote(
        RemoteMotion {
            pose: [8.0, 2.0],
            velocity: [0.0, 0.0],
            equipment: present_one(EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
        },
        Facing::Right,
    );
    let mut set = CharacterPresentationSet::new();
    set.sync([(key, a), (other, b)], &registry, 0.0);
    assert_eq!(set.len(), 2);
    set.sync([(key, a)], &registry, 0.0);
    assert!(set.get(other).is_none());
    assert_eq!(set.get(key).unwrap().bound().len(), 1);
}

#[test]
fn anchors_are_rig_constants_not_ad_hoc() {
    assert_eq!(
        anchor_local(AnchorPoint::BoneOrigin),
        BoneTransform::IDENTITY
    );
    assert_eq!(
        anchor_local(AnchorPoint::GripFront),
        anchor_local(AnchorPoint::GripBack)
    );
    assert_eq!(
        anchor_local(AnchorPoint::FootFront),
        anchor_local(AnchorPoint::FootBack)
    );
    assert_eq!(
        anchor_local(AnchorPoint::Crown),
        purgatory_skeleton::ANCHOR_CROWN
    );
    assert_eq!(
        anchor_local(AnchorPoint::Chest),
        purgatory_skeleton::ANCHOR_CHEST
    );
}
