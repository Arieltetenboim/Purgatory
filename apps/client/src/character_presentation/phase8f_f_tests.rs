//! Phase 8F-F closeout lock: canonical 8F path. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{
    BoneTarget, ContentRegistry, LoadMode, default_content_root, load_registry,
};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;

use super::adapters::{from_local, from_remote};
use super::collection::clip_for_playback_activity;
use super::compose::compose_attachment;
use super::draw_order::{PresentationLayer, layer_for_attachment, plan_character_draw};
use super::resolve::resolve_equipment;
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

#[test]
fn protocol_unchanged_by_8f() {
    assert_eq!(PROTOCOL_VERSION, 24);
}

#[test]
fn one_canonical_layer_table() {
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
fn climb_back_view_clip_and_visual_key_chain() {
    assert_eq!(
        view_for_activity(PresentationActivity::ClimbBack),
        PresentationView::Back
    );
    assert!(std::ptr::eq(
        clip_for_playback_activity(PresentationActivity::ClimbBack),
        purgatory_animation::climb_back_clip()
    ));
    assert!(!std::ptr::eq(
        clip_for_playback_activity(PresentationActivity::ClimbBack),
        clip_for_playback_activity(PresentationActivity::Idle)
    ));

    let registry = pack();
    let map = super::bone_map::BoneTargetMap::bind_humanoid_v0().unwrap();
    let out = resolve_equipment(
        present_slots(&[
            (EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap")),
            (EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
        ]),
        &registry,
        map,
    );
    let crown = out
        .bound
        .iter()
        .find(|b| b.attachment_id == "crown")
        .unwrap();
    let glove_back = out
        .bound
        .iter()
        .find(|b| b.attachment_id == "hand_back")
        .unwrap();
    assert_eq!(
        crown.visual_key_for_view(PresentationView::Side),
        Some("equipment.debug.cloth_cap.side")
    );
    assert_eq!(
        crown.visual_key_for_view(PresentationView::Back),
        Some("equipment.debug.cloth_cap.back")
    );
    assert_eq!(glove_back.visual_key_for_view(PresentationView::Back), None);
    let back_plan = plan_character_draw(0, &out.bound, PresentationView::Back);
    let back_ids: Vec<_> = back_plan
        .iter()
        .filter_map(|k| match k {
            super::draw_order::PlannedKind::Attachment(i) => {
                Some(out.bound[*i].attachment_id.as_str())
            }
            super::draw_order::PlannedKind::Base(_) => None,
        })
        .collect();
    assert!(back_ids.contains(&"crown"));
    assert!(!back_ids.contains(&"hand_back"));
    assert_eq!(
        layer_for_attachment(crown),
        super::draw_order::layer_for_bone_target(BoneTarget::Head)
    );
}

#[test]
fn local_and_remote_share_presentation_path() {
    let registry = pack();
    let equipment = present_slots(&[(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap"))]);
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
    assert_eq!(local.state().view, remote.state().view);
    assert_eq!(local.playback_activity(), remote.playback_activity());
    let lp = plan_character_draw(local.hidden_base(), local.bound(), local.state().view);
    let rp = plan_character_draw(remote.hidden_base(), remote.bound(), remote.state().view);
    assert_eq!(lp, rp);
    assert_eq!(local.bound()[0].anchor, remote.bound()[0].anchor);
    assert_eq!(local.hidden_base(), remote.hidden_base());
    let xf = compose_attachment(&local.bound()[0], local.prepared().world);
    assert!(xf.is_some());
}
