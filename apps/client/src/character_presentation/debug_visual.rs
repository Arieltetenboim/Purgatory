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
use super::draw_order::{PlannedKind, plan_character_draw};
use super::resolve::BoundAttachment;
use super::state::PresentationView;

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
    presentation_debug_quads_with_headwear(bone_map, entry, preview_scale, draw_body, view, 0)
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
                if let Some(quad) =
                    character_placeholder_quad(world, preview_scale, bone, piece.torso_far())
                {
                    quads.push(quad);
                }
            }
            PlannedKind::Attachment(i) => {
                let Some(bound) = entry.bound().get(i) else {
                    continue;
                };
                if let Some(quad) =
                    attachment_debug_quad(bound, world, preview_scale, view, headwear_cell)
                {
                    quads.push(quad);
                }
            }
        }
    }
    quads
}

fn attachment_debug_quad(
    bound: &BoundAttachment,
    world: &purgatory_skeleton::WorldPose,
    preview_scale: f32,
    view: PresentationView,
    headwear_cell: u8,
) -> Option<DrawQuad> {
    let key = bound.visual_key_for_view(view)?;
    let xf = compose_attachment(bound, world)?;
    let scale = crate::skeleton_debug::sanitize_preview_scale(preview_scale);
    let root = world.get(ROOT).map(|t| t.translation).unwrap_or([0.0, 0.0]);
    if uses_headwear_side_sprite(bound, view) {
        return Some(crate::headwear_proof::sprite_quad_for_cell(
            xf,
            preview_scale,
            root,
            headwear_cell,
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

/// Side Headwear Crown uses the compile-embedded Lab PNG. Back stays a Cap
/// placeholder (8F-E: do not Side-as-Back).
fn uses_headwear_side_sprite(bound: &BoundAttachment, view: PresentationView) -> bool {
    bound.slot == EquipmentSlot::Headwear
        && bound.anchor == AnchorPoint::Crown
        && view == PresentationView::Side
}
