//! DEV attachment debug quads. Shape from attachment semantics; color from the
//! visual key selected by [`PresentationView`]. Side Headwear Crown uses the
//! compile-embedded Lab sprite instead of a Cap placeholder.

use purgatory_content::{AnchorPoint, BoneTarget, CoverageMode};
use purgatory_simulation::EquipmentSlot;
use purgatory_skeleton::ROOT;

use crate::renderer::DrawQuad;
use crate::skeleton_debug::{character_placeholder_quad, scale_about_root};

use super::bone_map::BoneTargetMap;
use super::collection::CharacterPresentationEntry;
use super::compose::compose_attachment;
use super::draw_order::{BasePiece, PlannedKind, plan_character_draw};
use super::resolve::BoundAttachment;
use super::state::Facing;
use super::state::PresentationView;
use crate::asset_runtime::{AssetRuntime, ResolvedVisual};
use crate::character_assets::CharacterVisualPack;

/// Deterministic placeholder shape from slot / anchor / coverage / bone — not the visual-key string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugShape {
    Cap,
    TorsoShell,
    LimbRect,
    HandRect,
    FootRect,
    Blade,
}

#[must_use]
pub fn debug_shape(
    slot: EquipmentSlot,
    anchor: AnchorPoint,
    coverage: CoverageMode,
    bone: BoneTarget,
) -> DebugShape {
    if coverage == CoverageMode::ReplaceBase {
        return DebugShape::TorsoShell;
    }
    match slot {
        EquipmentSlot::Weapon
            if matches!(anchor, AnchorPoint::GripFront | AnchorPoint::GripBack) =>
        {
            DebugShape::Blade
        }
        EquipmentSlot::Headwear => DebugShape::Cap,
        EquipmentSlot::Boots => DebugShape::FootRect,
        EquipmentSlot::Gloves => DebugShape::HandRect,
        EquipmentSlot::Bodywear
            if matches!(anchor, AnchorPoint::Chest) || bone == BoneTarget::Torso =>
        {
            DebugShape::TorsoShell
        }
        _ if matches!(anchor, AnchorPoint::FootFront | AnchorPoint::FootBack) => {
            DebugShape::FootRect
        }
        _ => DebugShape::LimbRect,
    }
}

#[must_use]
pub fn color_from_visual_key(key: &str) -> [f32; 4] {
    let mut h = 0x811c_9dc5u32;
    for byte in key.as_bytes() {
        h ^= u32::from(*byte);
        h = h.wrapping_mul(0x0100_0193);
    }
    let r = 0.28 + f32::from(h as u8) / 255.0 * 0.62;
    let g = 0.22 + f32::from((h >> 8) as u8) / 255.0 * 0.62;
    let b = 0.18 + f32::from((h >> 16) as u8) / 255.0 * 0.70;
    [r, g, b, 0.92]
}

/// Shared local/remote presentation draw, in [`plan_character_draw`] order.
/// `view` is the semantic Front/Back policy (overlay may force Back for 8F-B proof).
#[must_use]
pub fn presentation_debug_quads(
    bone_map: BoneTargetMap,
    entry: &CharacterPresentationEntry,
    preview_scale: f32,
    draw_body: bool,
    view: PresentationView,
) -> Vec<DrawQuad> {
    let mut assets = crate::asset_runtime::AssetRuntime::new();
    if crate::headwear_proof::register_assets(&mut assets).is_err() {
        return Vec::new();
    }
    let Ok(visual_pack) = crate::character_assets::embedded_character_visual_pack(&mut assets)
    else {
        return Vec::new();
    };
    presentation_debug_quads_with_assets(
        bone_map,
        entry,
        &assets,
        &visual_pack,
        preview_scale,
        draw_body,
        view,
        0,
    )
}

/// Same as [`presentation_debug_quads`], with Headwear Side atlas cell `0..=3`.
#[must_use]
pub fn presentation_debug_quads_with_headwear(
    bone_map: BoneTargetMap,
    entry: &CharacterPresentationEntry,
    preview_scale: f32,
    draw_body: bool,
    view: PresentationView,
    headwear_cell: u8,
) -> Vec<DrawQuad> {
    let mut assets = crate::asset_runtime::AssetRuntime::new();
    if crate::headwear_proof::register_assets(&mut assets).is_err() {
        return Vec::new();
    }
    let Ok(visual_pack) = crate::character_assets::embedded_character_visual_pack(&mut assets)
    else {
        return Vec::new();
    };
    presentation_debug_quads_with_assets(
        bone_map,
        entry,
        &assets,
        &visual_pack,
        preview_scale,
        draw_body,
        view,
        headwear_cell,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn presentation_debug_quads_with_assets(
    bone_map: BoneTargetMap,
    entry: &CharacterPresentationEntry,
    assets: &AssetRuntime,
    visual_pack: &CharacterVisualPack,
    preview_scale: f32,
    draw_body: bool,
    view: PresentationView,
    headwear_cell: u8,
) -> Vec<DrawQuad> {
    if !draw_body {
        return Vec::new();
    }
    let prepared = entry.prepared();
    let world = prepared.world;
    let plan = plan_character_draw(entry.hidden_base(), entry.bound(), view);
    let mut quads = Vec::with_capacity(plan.len());
    for kind in plan {
        match kind {
            PlannedKind::Base(piece) => {
                let bone = bone_map.bone(piece.hide_target());
                let quad = if view == PresentationView::Side {
                    textured_base_quad(visual_pack, world, preview_scale, piece, bone).or_else(
                        || {
                            if piece.torso_far() {
                                None
                            } else {
                                character_placeholder_quad(world, preview_scale, bone, false)
                            }
                        },
                    )
                } else {
                    character_placeholder_quad(world, preview_scale, bone, piece.torso_far())
                };
                if let Some(quad) = quad {
                    quads.push(if view == PresentationView::Side {
                        mirror_for_facing(quad, entry.state().facing, world)
                    } else {
                        quad
                    });
                }
            }
            PlannedKind::Attachment(i) => {
                let Some(bound) = entry.bound().get(i) else {
                    continue;
                };
                if let Some(mut quad) = attachment_debug_quad(
                    assets,
                    bound,
                    world,
                    preview_scale,
                    view,
                    if headwear_cell == 0 {
                        None
                    } else {
                        crate::headwear_proof::VISUAL_KEYS
                            .get(usize::from(headwear_cell))
                            .copied()
                    },
                ) {
                    if entry.state().facing == Facing::Left {
                        let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
                        quad = quad.mirror_x_about(root);
                    }
                    quads.push(quad);
                }
            }
        }
    }
    quads
}

fn textured_base_quad(
    visual_pack: &CharacterVisualPack,
    world: &purgatory_skeleton::WorldPose,
    preview_scale: f32,
    piece: BasePiece,
    bone: purgatory_skeleton::BoneIndex,
) -> Option<DrawQuad> {
    let key = match piece {
        BasePiece::UpperArmBack => "character.base.dev_01.upper_arm_back.side",
        BasePiece::LowerArmBack => "character.base.dev_01.lower_arm_back.side",
        BasePiece::HandBack => "character.base.dev_01.hand_back.side",
        BasePiece::UpperLegBack => "character.base.dev_01.upper_leg_back.side",
        BasePiece::LowerLegBack => "character.base.dev_01.lower_leg_back.side",
        BasePiece::TorsoNear => "character.base.dev_01.torso.side",
        BasePiece::UpperLegFront => "character.base.dev_01.upper_leg_front.side",
        BasePiece::LowerLegFront => "character.base.dev_01.lower_leg_front.side",
        BasePiece::Head => "character.base.dev_01.head.side",
        BasePiece::UpperArmFront => "character.base.dev_01.upper_arm_front.side",
        BasePiece::LowerArmFront => "character.base.dev_01.lower_arm_front.side",
        BasePiece::HandFront => "character.base.dev_01.hand_front.side",
        BasePiece::FootBack | BasePiece::FootFront | BasePiece::TorsoFar => return None,
    };
    let visual = *visual_pack.visual(key)?;
    let xf = world.get(bone)?;
    let scale = crate::skeleton_debug::sanitize_preview_scale(preview_scale);
    let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
    let pivot = scale_about_root(xf.translation, root, scale);
    let quad = DrawQuad::textured_sprite(
        visual.texture,
        pivot,
        sprite_local_corners(visual, scale),
        visual.uv,
        xf.rotation,
    );
    Some(quad)
}

#[must_use]
fn mirror_for_facing(
    quad: DrawQuad,
    facing: Facing,
    world: &purgatory_skeleton::WorldPose,
) -> DrawQuad {
    if facing == Facing::Left {
        let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
        quad.mirror_x_about(root)
    } else {
        quad
    }
}

fn sprite_local_corners(visual: ResolvedVisual, scale: f32) -> [[f32; 2]; 4] {
    let [width, height] = visual.dimensions_px;
    let ppu = visual.pixels_per_unit;
    let [pivot_x, pivot_y] = visual.pivot_px;
    let sx = scale / ppu;
    [
        [-pivot_x * sx, (pivot_y - height as f32) * sx],
        [
            (width as f32 - pivot_x) * sx,
            (pivot_y - height as f32) * sx,
        ],
        [(width as f32 - pivot_x) * sx, pivot_y * sx],
        [-pivot_x * sx, pivot_y * sx],
    ]
}

fn attachment_debug_quad(
    assets: &AssetRuntime,
    bound: &BoundAttachment,
    world: &purgatory_skeleton::WorldPose,
    preview_scale: f32,
    view: PresentationView,
    visual_key_override: Option<&str>,
) -> Option<DrawQuad> {
    let key = visual_key_override.or_else(|| bound.visual_key_for_view(view))?;
    let xf = compose_attachment(bound, world)?;
    let scale = crate::skeleton_debug::sanitize_preview_scale(preview_scale);
    let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
    if let Some(visual) = assets.visual(key) {
        let pivot = scale_about_root(xf.translation, root, scale);
        return Some(DrawQuad::textured_sprite(
            visual.texture,
            pivot,
            sprite_local_corners(*visual, scale),
            visual.uv,
            xf.rotation,
        ));
    }
    let shape = debug_shape(bound.slot, bound.anchor, bound.coverage, bound.bone_target);
    let color = color_from_visual_key(key);
    let pivot = scale_about_root(xf.translation, root, scale);
    let (size, local_center) = match shape {
        DebugShape::Blade => {
            let size = [0.34 * scale, 0.05 * scale];
            (size, [size[0] * 0.5, 0.0])
        }
        DebugShape::Cap => {
            let size = [0.13 * scale, 0.07 * scale];
            (size, [0.0, size[1] * 0.45])
        }
        DebugShape::TorsoShell => {
            let size = [0.22 * scale, 0.28 * scale];
            (size, [0.0, size[1] * 0.15])
        }
        DebugShape::FootRect => {
            let size = [0.14 * scale, 0.06 * scale];
            (size, [size[0] * 0.35, 0.0])
        }
        DebugShape::HandRect => {
            let size = [0.08 * scale, 0.09 * scale];
            (size, [0.0, -size[1] * 0.35])
        }
        DebugShape::LimbRect => {
            let size = [0.10 * scale, 0.18 * scale];
            (size, [0.0, -size[1] * 0.45])
        }
    };
    Some(DrawQuad::oriented(
        pivot,
        size,
        local_center,
        xf.rotation,
        color,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_skeleton::{LocalPose, UPPER_ARM_BACK, WorldPose, evaluate, humanoid_v0};

    fn pack() -> (crate::asset_runtime::AssetRuntime, CharacterVisualPack) {
        let mut assets = crate::asset_runtime::AssetRuntime::new();
        let pack = crate::character_assets::embedded_character_visual_pack(&mut assets).unwrap();
        (assets, pack)
    }

    fn bind_world() -> WorldPose {
        let def = humanoid_v0();
        let local = LocalPose::from_bind(def);
        let mut world = WorldPose::new(def);
        evaluate(def, &local, &mut world).unwrap();
        world
    }

    #[test]
    fn exported_base_part_is_textured_and_uses_existing_bone_transform() {
        let (_assets, pack) = pack();
        let world = bind_world();
        let quad = textured_base_quad(&pack, &world, 1.0, BasePiece::UpperArmBack, UPPER_ARM_BACK)
            .unwrap();
        let visual = pack
            .visual("character.base.dev_01.upper_arm_back.side")
            .unwrap();
        assert_eq!(quad.sprite_texture_id(), Some(visual.texture));
        assert_eq!(quad.uvs(), visual.uv);
        assert_eq!(
            quad.world_corners()[0][0] - world.get(UPPER_ARM_BACK).unwrap().translation[0],
            -visual.pivot_px[0] / visual.pixels_per_unit
        );
    }

    #[test]
    fn sprite_geometry_uses_negative_pivot_and_ppu() {
        let (_assets, pack) = pack();
        let visual = *pack
            .visual("character.base.dev_01.upper_arm_back.side")
            .unwrap();
        let corners = sprite_local_corners(visual, 1.0);
        assert_eq!(
            corners,
            [
                [
                    -visual.pivot_px[0] / 256.0,
                    (visual.pivot_px[1] - 51.0) / 256.0
                ],
                [
                    (46.0 - visual.pivot_px[0]) / 256.0,
                    (visual.pivot_px[1] - 51.0) / 256.0
                ],
                [
                    (46.0 - visual.pivot_px[0]) / 256.0,
                    visual.pivot_px[1] / 256.0
                ],
                [-visual.pivot_px[0] / 256.0, visual.pivot_px[1] / 256.0],
            ]
        );
    }

    #[test]
    fn partial_pack_does_not_fabricate_feet_or_duplicate_torso() {
        let (_assets, pack) = pack();
        let world = bind_world();
        let bone = purgatory_skeleton::TORSO;
        assert!(textured_base_quad(&pack, &world, 1.0, BasePiece::TorsoNear, bone,).is_some());
        assert!(textured_base_quad(&pack, &world, 1.0, BasePiece::TorsoFar, bone,).is_none());
        assert!(
            textured_base_quad(
                &pack,
                &world,
                1.0,
                BasePiece::FootFront,
                purgatory_skeleton::FOOT_FRONT,
            )
            .is_none()
        );
    }

    #[test]
    fn left_facing_sprite_mirrors_about_root_without_pose_change() {
        let (_assets, pack) = pack();
        let world = bind_world();
        let root = world.get(ROOT).unwrap().translation;
        let right = textured_base_quad(&pack, &world, 1.0, BasePiece::UpperArmBack, UPPER_ARM_BACK)
            .unwrap();
        let left = textured_base_quad(&pack, &world, 1.0, BasePiece::UpperArmBack, UPPER_ARM_BACK)
            .unwrap();
        let left = mirror_for_facing(left, Facing::Left, &world);
        let right_corners = right.world_corners();
        let left_corners = left.world_corners();
        for (right, left) in [1, 0, 3, 2]
            .into_iter()
            .map(|index| right_corners[index])
            .zip(left_corners)
        {
            assert!((left[0] - (root[0] * 2.0 - right[0])).abs() < 1e-6);
            assert!((left[1] - right[1]).abs() < 1e-6);
        }
    }

    #[test]
    fn left_facing_head_mirrors_geometry_and_uvs_about_root() {
        let (_assets, pack) = pack();
        let world = bind_world();
        let root = world.get(ROOT).unwrap().translation;
        let right = textured_base_quad(
            &pack,
            &world,
            1.0,
            BasePiece::Head,
            purgatory_skeleton::HEAD,
        )
        .unwrap();
        let left = textured_base_quad(
            &pack,
            &world,
            1.0,
            BasePiece::Head,
            purgatory_skeleton::HEAD,
        )
        .unwrap();
        let left = mirror_for_facing(left, Facing::Left, &world);

        for (right, left) in [1, 0, 3, 2]
            .into_iter()
            .map(|index| right.world_corners()[index])
            .zip(left.world_corners())
        {
            assert!((left[0] - (root[0] * 2.0 - right[0])).abs() < 1e-6);
            assert!((left[1] - right[1]).abs() < 1e-6);
        }
        assert_eq!(
            left.uvs(),
            [
                right.uvs()[1],
                right.uvs()[0],
                right.uvs()[3],
                right.uvs()[2]
            ]
        );
    }

    #[test]
    fn head_exported_pivot_maps_to_head_bone_without_runtime_offset() {
        let (_assets, pack) = pack();
        let world = bind_world();
        let head = world.get(purgatory_skeleton::HEAD).unwrap();
        let visual = *pack.visual("character.base.dev_01.head.side").unwrap();
        let quad = textured_base_quad(
            &pack,
            &world,
            1.0,
            BasePiece::Head,
            purgatory_skeleton::HEAD,
        )
        .unwrap();
        let corners = quad.world_corners();
        let bottom_y = (corners[0][1] + corners[1][1]) * 0.5;
        let scale = 1.0;
        let mapped_pivot_y = bottom_y
            - scale * (visual.pivot_px[1] - visual.dimensions_px[1] as f32)
                / visual.pixels_per_unit;
        let scaled_head_y = scale * head.translation[1];
        assert!((mapped_pivot_y - scaled_head_y).abs() < 1e-4);
    }

    #[test]
    fn foot_fallbacks_mirror_about_root_under_rotated_pose() {
        let def = humanoid_v0();
        let mut local = LocalPose::from_bind(def);
        local.get_mut(ROOT).unwrap().translation = [2.0, -1.0];
        local
            .get_mut(purgatory_skeleton::UPPER_LEG_FRONT)
            .unwrap()
            .rotation = 0.35;
        local
            .get_mut(purgatory_skeleton::LOWER_LEG_FRONT)
            .unwrap()
            .rotation = -0.55;
        local
            .get_mut(purgatory_skeleton::FOOT_FRONT)
            .unwrap()
            .rotation = 0.4;
        local
            .get_mut(purgatory_skeleton::UPPER_LEG_BACK)
            .unwrap()
            .rotation = -0.3;
        local
            .get_mut(purgatory_skeleton::LOWER_LEG_BACK)
            .unwrap()
            .rotation = 0.5;
        local
            .get_mut(purgatory_skeleton::FOOT_BACK)
            .unwrap()
            .rotation = -0.45;
        let mut world = WorldPose::new(def);
        evaluate(def, &local, &mut world).unwrap();

        let root = world.get(ROOT).unwrap().translation;
        for bone in [
            purgatory_skeleton::FOOT_FRONT,
            purgatory_skeleton::FOOT_BACK,
        ] {
            let right = character_placeholder_quad(&world, 1.0, bone, false).unwrap();
            let left = mirror_for_facing(
                character_placeholder_quad(&world, 1.0, bone, false).unwrap(),
                Facing::Left,
                &world,
            );
            for (right, left) in [1, 0, 3, 2]
                .into_iter()
                .map(|index| right.world_corners()[index])
                .zip(left.world_corners())
            {
                assert!((left[0] - (root[0] * 2.0 - right[0])).abs() < 1e-6);
                assert!((left[1] - right[1]).abs() < 1e-6);
            }
        }
    }
}
