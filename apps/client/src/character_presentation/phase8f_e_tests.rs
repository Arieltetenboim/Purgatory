//! Phase 8F-E equipment Side/Back visual keys from PresentationView. No wgpu / window.

use purgatory_common::ContentId;
use purgatory_content::{ContentRegistry, LoadMode, default_content_root, load_registry};
use purgatory_protocol::PROTOCOL_VERSION;
use purgatory_simulation::EquipmentSlot;
use purgatory_skeleton::HEAD;

use super::adapters::{apply_climb_back_overlay, from_local, from_remote};
use super::compose::compose_attachment;
use super::debug_visual::{
    color_from_visual_key, presentation_debug_quads, presentation_debug_quads_with_headwear,
};
use super::draw_order::{layer_for_attachment, plan_character_draw};
use super::presentation_debug_quads_with_assets;
use super::resolve::resolve_equipment;
use super::state::{EquipmentView, Facing, PresentationView};
use super::{CharacterPresentationSet, LocalMotion, PresentationEntityKey, RemoteMotion};

fn pack() -> ContentRegistry {
    load_registry(&default_content_root(), LoadMode::Shared).expect("shared content")
}

fn cid(authored: &str) -> ContentId {
    purgatory_common::allocated_id_for_label(authored).expect("catalog content")
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
fn protocol_unchanged() {
    assert_eq!(PROTOCOL_VERSION, 35);
}

#[test]
fn side_view_selects_side_visual_key() {
    let registry = pack();
    let map = super::bone_map::BoneTargetMap::bind_humanoid_v0().unwrap();
    let out = resolve_equipment(
        present_slots(&[(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap"))]),
        &registry,
        map,
    );
    let crown = &out.bound[0];
    assert_eq!(
        crown.visual_key_for_view(PresentationView::Side),
        Some("equipment.debug.cloth_cap.side")
    );
    assert_eq!(crown.visual_key_side, "equipment.debug.cloth_cap.side");
}

#[test]
fn back_view_selects_back_visual_key_when_authored() {
    let registry = pack();
    let map = super::bone_map::BoneTargetMap::bind_humanoid_v0().unwrap();
    let out = resolve_equipment(
        present_slots(&[
            (EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap")),
            (
                EquipmentSlot::Bodywear,
                cid("equipment.debug.plate_cuirass"),
            ),
            (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
        ]),
        &registry,
        map,
    );
    let crown = out
        .bound
        .iter()
        .find(|b| b.attachment_id == "crown")
        .unwrap();
    let shell = out
        .bound
        .iter()
        .find(|b| b.attachment_id == "shell")
        .unwrap();
    let foot_back = out
        .bound
        .iter()
        .find(|b| b.attachment_id == "foot_back")
        .unwrap();
    assert_eq!(
        crown.visual_key_for_view(PresentationView::Back),
        Some("equipment.debug.cloth_cap.back")
    );
    assert_eq!(
        shell.visual_key_for_view(PresentationView::Back),
        Some("equipment.debug.plate_cuirass.back")
    );
    assert_eq!(
        foot_back.visual_key_for_view(PresentationView::Back),
        Some("equipment.debug.iron_boots.foot_back.back")
    );
    assert_ne!(
        crown.visual_key_for_view(PresentationView::Side),
        crown.visual_key_for_view(PresentationView::Back)
    );
}

#[test]
fn missing_back_visual_does_not_substitute_side() {
    let registry = pack();
    let map = super::bone_map::BoneTargetMap::bind_humanoid_v0().unwrap();
    let out = resolve_equipment(
        present_slots(&[(EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves"))]),
        &registry,
        map,
    );
    let hand_back = out
        .bound
        .iter()
        .find(|b| b.attachment_id == "hand_back")
        .unwrap();
    assert!(hand_back.visual_key_back.is_none());
    assert_eq!(
        hand_back.visual_key_for_view(PresentationView::Side),
        Some("equipment.debug.leather_gloves.hand_back.side")
    );
    assert_eq!(hand_back.visual_key_for_view(PresentationView::Back), None);

    let side = plan_character_draw(0, &out.bound, PresentationView::Side);
    let back = plan_character_draw(0, &out.bound, PresentationView::Back);
    let side_ids: Vec<_> = side
        .iter()
        .filter_map(|k| match k {
            super::draw_order::PlannedKind::Attachment(i) => {
                Some(out.bound[*i].attachment_id.as_str())
            }
            super::draw_order::PlannedKind::Base(_) => None,
        })
        .collect();
    let back_ids: Vec<_> = back
        .iter()
        .filter_map(|k| match k {
            super::draw_order::PlannedKind::Attachment(i) => {
                Some(out.bound[*i].attachment_id.as_str())
            }
            super::draw_order::PlannedKind::Base(_) => None,
        })
        .collect();
    assert!(side_ids.contains(&"hand_back"));
    assert!(!back_ids.contains(&"hand_back"));
    assert!(!back_ids.contains(&"hand_front"));
}

#[test]
fn local_and_remote_select_the_same_visual_keys() {
    let registry = pack();
    let equipment = present_slots(&[
        (EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap")),
        (EquipmentSlot::Gloves, cid("equipment.debug.leather_gloves")),
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
    assert_eq!(local.state().view, PresentationView::Back);
    assert_eq!(remote.state().view, PresentationView::Back);
    let l_cap = local
        .bound()
        .iter()
        .find(|b| b.attachment_id == "crown")
        .unwrap();
    let r_cap = remote
        .bound()
        .iter()
        .find(|b| b.attachment_id == "crown")
        .unwrap();
    assert_eq!(
        l_cap.visual_key_for_view(local.state().view),
        r_cap.visual_key_for_view(remote.state().view)
    );
    assert_eq!(
        l_cap.visual_key_for_view(PresentationView::Back),
        Some("equipment.debug.cloth_cap.back")
    );
    let lp = plan_character_draw(local.hidden_base(), local.bound(), local.state().view);
    let rp = plan_character_draw(remote.hidden_base(), remote.bound(), remote.state().view);
    assert_eq!(lp, rp);
    let lq = presentation_debug_quads(set.bone_map(), local, 1.15, true, local.state().view);
    let rq = presentation_debug_quads(set.bone_map(), remote, 1.15, true, remote.state().view);
    assert_eq!(lq.len(), rq.len());
    assert_eq!(lq.len(), lp.len());
}

#[test]
fn visual_key_switch_does_not_change_transform_or_layer() {
    let registry = pack();
    let map = super::bone_map::BoneTargetMap::bind_humanoid_v0().unwrap();
    let equipment = present_slots(&[(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap"))]);
    let out = resolve_equipment(equipment, &registry, map);
    let crown = &out.bound[0];
    let layer = layer_for_attachment(crown);
    assert_eq!(crown.bone, HEAD);
    assert_eq!(crown.anchor, purgatory_content::AnchorPoint::Crown);

    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(1, 1);
    set.sync([(key, idle(equipment))], &registry, 0.0);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.state().view, PresentationView::Side);
    let world = entry.prepared().world;
    let xf_side = compose_attachment(crown, world).unwrap();
    let xf_back = compose_attachment(crown, world).unwrap();
    assert_eq!(xf_side, xf_back);
    assert_eq!(layer_for_attachment(&entry.bound()[0]), layer);

    let side_key = crown.visual_key_for_view(PresentationView::Side).unwrap();
    let back_key = crown.visual_key_for_view(PresentationView::Back).unwrap();
    assert_ne!(side_key, back_key);
    assert_ne!(
        color_from_visual_key(side_key),
        color_from_visual_key(back_key)
    );

    let side_plan = plan_character_draw(0, &out.bound, PresentationView::Side);
    let back_plan = plan_character_draw(0, &out.bound, PresentationView::Back);
    assert!(
        side_plan
            .iter()
            .any(|k| matches!(k, super::draw_order::PlannedKind::Attachment(0)))
    );
    assert!(
        back_plan
            .iter()
            .any(|k| matches!(k, super::draw_order::PlannedKind::Attachment(0)))
    );
}

#[test]
fn draw_view_selects_key_not_climb_activity() {
    let registry = pack();
    let equipment = present_slots(&[(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap"))]);
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(2, 1);
    set.sync([(key, idle(equipment))], &registry, 0.0);
    let entry = set.get(key).unwrap();
    assert_eq!(entry.state().view, PresentationView::Side);
    assert_ne!(
        entry.state().activity,
        super::state::PresentationActivity::ClimbBack
    );
    let crown = entry
        .bound()
        .iter()
        .find(|b| b.attachment_id == "crown")
        .unwrap();
    assert_eq!(
        crown.visual_key_for_view(PresentationView::Side),
        Some("equipment.debug.cloth_cap.side")
    );
    assert_eq!(
        crown.visual_key_for_view(PresentationView::Back),
        Some("equipment.debug.cloth_cap.back")
    );
    let side_plan = plan_character_draw(entry.hidden_base(), entry.bound(), PresentationView::Side);
    let back_plan = plan_character_draw(entry.hidden_base(), entry.bound(), PresentationView::Back);
    let side_quads =
        presentation_debug_quads(set.bone_map(), entry, 1.15, true, PresentationView::Side);
    let back_quads =
        presentation_debug_quads(set.bone_map(), entry, 1.15, true, PresentationView::Back);
    let crown_side_i = side_plan
        .iter()
        .position(|k| match k {
            super::draw_order::PlannedKind::Attachment(i) => {
                entry.bound()[*i].attachment_id == "crown"
            }
            super::draw_order::PlannedKind::Base(_) => false,
        })
        .unwrap();
    let crown_back_i = back_plan
        .iter()
        .position(|k| match k {
            super::draw_order::PlannedKind::Attachment(i) => {
                entry.bound()[*i].attachment_id == "crown"
            }
            super::draw_order::PlannedKind::Base(_) => false,
        })
        .unwrap();
    assert_ne!(
        side_quads[crown_side_i].color, back_quads[crown_back_i].color,
        "debug color follows selected visual key"
    );
    assert!(
        side_quads[crown_side_i].is_textured(),
        "Side Headwear Crown uses the Lab sprite"
    );
    assert!(
        !back_quads[crown_back_i].is_textured(),
        "Back must not Side-as-Back the Lab sprite"
    );
}

#[test]
fn headwear_side_sprite_is_shared_local_remote_path() {
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
    let map = set.bone_map();
    let lq = presentation_debug_quads(map, local, 1.15, true, PresentationView::Side);
    let rq = presentation_debug_quads(map, remote, 1.15, true, PresentationView::Side);
    let l_crown_i = lq
        .iter()
        .position(|quad| {
            quad.sprite_texture_id() == Some(crate::renderer::SpriteTextureId::HEADWEAR)
        })
        .unwrap();
    let r_crown_i = rq
        .iter()
        .position(|quad| {
            quad.sprite_texture_id() == Some(crate::renderer::SpriteTextureId::HEADWEAR)
        })
        .unwrap();
    assert!(lq[l_crown_i].is_textured());
    assert!(rq[r_crown_i].is_textured());
    let l_crown = local
        .bound()
        .iter()
        .find(|b| b.attachment_id == "crown")
        .unwrap();
    let xf = compose_attachment(l_crown, local.prepared().world).unwrap();
    let root = local
        .prepared()
        .world
        .get(purgatory_skeleton::ROOT)
        .map(|t| t.translation)
        .unwrap_or([0.0, 0.0]);
    let expected = crate::headwear_proof::sprite_quad_for_cell(xf, 1.15, root, 0);
    assert_eq!(lq[l_crown_i].world_corners(), expected.world_corners());
    assert_eq!(
        xf.rotation,
        local.prepared().world.get(HEAD).unwrap().rotation
    );
}

#[test]
fn left_facing_headwear_mirrors_around_root_and_stays_on_crown() {
    let registry = pack();
    let equipment = present_slots(&[(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap"))]);
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(3, 1);
    set.sync(
        [(
            key,
            from_local(
                LocalMotion {
                    pose: [0.0, 0.0],
                    velocity: [0.0, 0.0],
                    grounded: true,
                    equipment,
                },
                Facing::Left,
            ),
        )],
        &registry,
        0.0,
    );
    let entry = set.get(key).unwrap();
    let quads = presentation_debug_quads(set.bone_map(), entry, 1.15, true, PresentationView::Side);
    let headwear = quads
        .iter()
        .find(|quad| quad.sprite_texture_id() == Some(crate::renderer::SpriteTextureId::HEADWEAR))
        .copied()
        .unwrap();
    let crown = entry
        .bound()
        .iter()
        .find(|bound| bound.attachment_id == "crown")
        .unwrap();
    let xf = compose_attachment(crown, entry.prepared().world).unwrap();
    let root = entry
        .prepared()
        .world
        .get(purgatory_skeleton::ROOT)
        .unwrap()
        .translation;
    let right = crate::headwear_proof::sprite_quad_for_cell(xf, 1.15, root, 0);
    assert_eq!(
        headwear.world_corners(),
        right.mirror_x_about(root).world_corners()
    );
    assert_eq!(headwear.uvs(), right.mirror_x_about(root).uvs());
    let crown_origin = xf.translation;
    assert!(headwear.world_corners().iter().any(|corner| {
        (corner[0] - crown_origin[0]).abs() < 1.0 && (corner[1] - crown_origin[1]).abs() < 1.0
    }));
}

#[test]
fn headwear_debug_cell_selects_atlas_uvs() {
    let registry = pack();
    let equipment = present_slots(&[(EquipmentSlot::Headwear, cid("equipment.debug.cloth_cap"))]);
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(3, 1);
    set.sync([(key, idle(equipment))], &registry, 0.0);
    let entry = set.get(key).unwrap();
    let map = set.bone_map();
    let cell0 =
        presentation_debug_quads_with_headwear(map, entry, 1.15, true, PresentationView::Side, 0);
    let cell1 =
        presentation_debug_quads_with_headwear(map, entry, 1.15, true, PresentationView::Side, 1);
    let crown_i = |quads: &[crate::renderer::DrawQuad]| {
        quads
            .iter()
            .position(|quad| {
                quad.sprite_texture_id() == Some(crate::renderer::SpriteTextureId::HEADWEAR)
            })
            .unwrap()
    };
    assert_eq!(
        cell0[crown_i(&cell0)].uvs(),
        crate::headwear_proof::gpu_uvs_for_cell(0)
    );
    assert_eq!(
        cell1[crown_i(&cell1)].uvs(),
        crate::headwear_proof::gpu_uvs_for_cell(1)
    );
    assert_ne!(cell0[crown_i(&cell0)].uvs(), cell1[crown_i(&cell1)].uvs());
}

fn sword_entry(
    registry: &ContentRegistry,
    facing: Facing,
) -> (CharacterPresentationSet, PresentationEntityKey) {
    let equipment = present_slots(&[
        (EquipmentSlot::Weapon, cid("equipment.debug.practice_sword")),
        (EquipmentSlot::Boots, cid("equipment.debug.iron_boots")),
    ]);
    let mut set = CharacterPresentationSet::new();
    let key = PresentationEntityKey::new(11, 1);
    set.sync(
        [(
            key,
            from_local(
                LocalMotion {
                    pose: [3.0, 1.0],
                    velocity: [0.0, 0.0],
                    grounded: true,
                    equipment,
                },
                facing,
            ),
        )],
        registry,
        0.0,
    );
    (set, key)
}

fn recover_pivot(quad: crate::renderer::DrawQuad, local: [f32; 2], rotation: f32) -> [f32; 2] {
    let world = quad.world_corners()[0];
    let (sin, cos) = rotation.sin_cos();
    let rotated = [
        cos * local[0] - sin * local[1],
        sin * local[0] + cos * local[1],
    ];
    [world[0] - rotated[0], world[1] - rotated[1]]
}

fn sprite_local_corner(visual: crate::asset_runtime::ResolvedVisual, scale: f32) -> [f32; 2] {
    let height = visual.dimensions_px[1] as f32;
    let sx = scale / visual.pixels_per_unit;
    [-visual.pivot_px[0] * sx, (visual.pivot_px[1] - height) * sx]
}

#[test]
fn practice_sword_uses_the_authored_visual_and_keeps_the_grip_on_the_hand() {
    let registry = pack();
    let (set, key) = sword_entry(&registry, Facing::Right);
    let entry = set.get(key).unwrap();
    let blade = entry
        .bound()
        .iter()
        .find(|bound| bound.attachment_id == "blade")
        .unwrap();
    assert_eq!(
        blade.visual_key_for_view(PresentationView::Side),
        Some(crate::practice_sword::VISUAL_KEY)
    );
    let mut assets = crate::asset_runtime::AssetRuntime::new();
    crate::headwear_proof::register_assets(&mut assets).unwrap();
    let visual_pack = crate::character_assets::embedded_character_visual_pack(&mut assets).unwrap();
    let unregistered = presentation_debug_quads_with_assets(
        set.bone_map(),
        entry,
        &assets,
        &visual_pack,
        1.0,
        true,
        PresentationView::Side,
        0,
    );
    assert_eq!(
        unregistered
            .iter()
            .filter(|quad| quad.sprite_texture_id().is_none())
            .count(),
        3,
        "sword plus both boots stay on the debug fallback until a visual is registered"
    );

    let hand = *visual_pack
        .visual("character.base.dev_01.hand_front.side")
        .unwrap();
    crate::practice_sword::register_assets(&mut assets, hand).unwrap();
    let visual = *assets.visual(crate::practice_sword::VISUAL_KEY).unwrap();
    let quads = presentation_debug_quads_with_assets(
        set.bone_map(),
        entry,
        &assets,
        &visual_pack,
        1.0,
        true,
        PresentationView::Side,
        0,
    );
    let sword = quads
        .iter()
        .copied()
        .find(|quad| quad.sprite_texture_id() == Some(visual.texture))
        .expect("equipped sword resolved through AssetRuntime");
    assert!(sword.is_textured());
    let xf = compose_attachment(blade, entry.prepared().world).unwrap();
    let root = entry
        .prepared()
        .world
        .get(purgatory_skeleton::ROOT)
        .unwrap()
        .translation;
    let grip = crate::skeleton_debug::scale_about_root(xf.translation, root, 1.0);
    let recovered = recover_pivot(sword, sprite_local_corner(visual, 1.0), xf.rotation);
    assert!((recovered[0] - grip[0]).abs() < 1e-3);
    assert!((recovered[1] - grip[1]).abs() < 1e-3);
    assert_eq!(
        quads
            .iter()
            .filter(|quad| quad.sprite_texture_id().is_none())
            .count(),
        2,
        "iron boots still fall back; the sword no longer uses DebugShape::Blade"
    );
}

#[test]
fn practice_sword_facing_left_mirrors_the_grip_about_root() {
    let registry = pack();
    let (right_set, right_key) = sword_entry(&registry, Facing::Right);
    let (left_set, left_key) = sword_entry(&registry, Facing::Left);
    let right = right_set.get(right_key).unwrap();
    let left = left_set.get(left_key).unwrap();
    let mut assets = crate::asset_runtime::AssetRuntime::new();
    crate::headwear_proof::register_assets(&mut assets).unwrap();
    let visual_pack = crate::character_assets::embedded_character_visual_pack(&mut assets).unwrap();
    let hand = *visual_pack
        .visual("character.base.dev_01.hand_front.side")
        .unwrap();
    crate::practice_sword::register_assets(&mut assets, hand).unwrap();
    let visual = *assets.visual(crate::practice_sword::VISUAL_KEY).unwrap();
    let draw = |set: &CharacterPresentationSet,
                entry: &super::collection::CharacterPresentationEntry| {
        presentation_debug_quads_with_assets(
            set.bone_map(),
            entry,
            &assets,
            &visual_pack,
            1.0,
            true,
            PresentationView::Side,
            0,
        )
    };
    let right_quads = draw(&right_set, right);
    let left_quads = draw(&left_set, left);
    let right_sword = right_quads
        .iter()
        .copied()
        .find(|quad| quad.sprite_texture_id() == Some(visual.texture))
        .unwrap();
    let left_sword = left_quads
        .iter()
        .copied()
        .find(|quad| quad.sprite_texture_id() == Some(visual.texture))
        .unwrap();
    let root = right
        .prepared()
        .world
        .get(purgatory_skeleton::ROOT)
        .unwrap()
        .translation;
    assert_eq!(
        left_sword.world_corners(),
        right_sword.mirror_x_about(root).world_corners()
    );
    let blade = right
        .bound()
        .iter()
        .find(|bound| bound.attachment_id == "blade")
        .unwrap();
    let xf = compose_attachment(blade, right.prepared().world).unwrap();
    let grip = crate::skeleton_debug::scale_about_root(xf.translation, root, 1.0);
    let mirrored = [root[0] * 2.0 - grip[0], grip[1]];
    let (sin, cos) = (-xf.rotation).sin_cos();
    let local = sprite_local_corner(visual, 1.0);
    let mirrored_local = [-local[0], local[1]];
    let rotated = [
        cos * mirrored_local[0] - sin * mirrored_local[1],
        sin * mirrored_local[0] + cos * mirrored_local[1],
    ];
    let left_corner = left_sword.world_corners()[1];
    let left_grip = [left_corner[0] - rotated[0], left_corner[1] - rotated[1]];
    assert!((left_grip[0] - mirrored[0]).abs() < 1e-3);
    assert!((left_grip[1] - mirrored[1]).abs() < 1e-3);
}
