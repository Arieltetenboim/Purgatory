//! AI-facing Humanoid v0 modular Paper-Doll reference (Hub-only).
//!
//! Same bind / slot-rest / anchor / torso-placeholder contracts as the technical
//! authoring template. Panel lengths come from child-span rest translations.
//! Panel widths match client P4 placeholders (visual only, not bind pose).
//! Does not replace [`crate::authoring_template`].

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use purgatory_skeleton::{
    ANCHOR_CHEST, ANCHOR_CROWN, ANCHOR_FOOT, ANCHOR_GRIP, BIND_FOOT_BACK, BIND_FOOT_FRONT,
    BIND_HAND_BACK, BIND_HAND_FRONT, BIND_LOWER_ARM_BACK, BIND_LOWER_ARM_FRONT,
    BIND_LOWER_LEG_BACK, BIND_LOWER_LEG_FRONT, BoneIndex, BoneTransform, FOOT_BACK, FOOT_FRONT,
    HAND_BACK, HAND_FRONT, HEAD, LOWER_ARM_BACK, LOWER_ARM_FRONT, LOWER_LEG_BACK, LOWER_LEG_FRONT,
    LocalPose, TORSO, UPPER_ARM_BACK, UPPER_ARM_FRONT, UPPER_LEG_BACK, UPPER_LEG_FRONT, WorldPose,
    evaluate, humanoid_v0, torso_far_local_corners, torso_local_corners,
};

use crate::authoring_template::{CanvasMap, PX_PER_WU, apply_local, fmt_num};

pub const CANVAS_W: u32 = 1600;
pub const CANVAS_H: u32 = 1024;
/// Assembled Side character: canvas pixel of world origin (root / ground).
pub const ASSEMBLED_ORIGIN: [f32; 2] = [280.0, 800.0];

const OUTPUT_REL: &str = "Graphic/character/HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg";

/// Visual panel widths / end-effector sizes. Must match `apps/client/src/skeleton_debug.rs` P4.
const ARM_WIDTH: f32 = 0.0944;
const LEG_WIDTH: f32 = 0.0976;
const HAND_SIZE: [f32; 2] = [0.084, 0.096];
const FOOT_SIZE: [f32; 2] = [0.12, 0.069];
const HEAD_BASE: [f32; 2] = [0.14, 0.16];
const HEAD_VISUAL_SCALE: f32 = 2.0;
const HEAD_WIDTH_SCALE: f32 = 1.10;

const FILL_BACK: &str = "#78716c";
const FILL_CORE: &str = "#a8a29e";
const FILL_FRONT: &str = "#d6d3d1";
const FILL_HEAD: &str = "#f5f5f4";
const FILL_FAR: &str = "#57534e";
const STROKE: &str = "#292524";
const ANCHOR: &str = "#b91c1c";

#[must_use]
pub fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(OUTPUT_REL)
}

#[must_use]
pub fn assembled_map() -> CanvasMap {
    CanvasMap {
        origin: ASSEMBLED_ORIGIN,
        px_per_wu: PX_PER_WU,
    }
}

/// Crown in presentation-world units (head ∘ `ANCHOR_CROWN`).
#[must_use]
pub fn crown_world() -> [f32; 2] {
    bind_world().crown().translation
}

/// Crown on the assembled Side figure (this sheet's canvas pixels).
#[must_use]
pub fn crown_assembled_canvas() -> [f32; 2] {
    assembled_map().to_canvas(crown_world())
}

pub fn export_to(path: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create dir: {err}"))?;
    }
    std::fs::write(path, render_svg()).map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

#[must_use]
pub fn render_svg() -> String {
    let pose = bind_world();
    let assembled = assembled_map();
    let mut out = String::with_capacity(16_000);
    let _ = writeln!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{CANVAS_W}\" height=\"{CANVAS_H}\" viewBox=\"0 0 {CANVAS_W} {CANVAS_H}\">\n\
  <title>PURGATORY Humanoid v0 AI modular reference</title>\n\
  <desc>Side Paper-Doll reference. Same Humanoid v0 bind/anchors as the runtime. \
Equipment is a separate visual. Headwear fits Crown. Do not redraw or fuse the character.</desc>"
    );

    let _ = writeln!(
        out,
        "  <g id=\"caption\" font-family=\"Segoe UI,Arial,sans-serif\" fill=\"#292524\">\n\
    <text x=\"40\" y=\"48\" font-size=\"22\" font-weight=\"700\">Humanoid v0 · Side</text>\n\
    <text x=\"40\" y=\"72\" font-size=\"14\" fill=\"#57534e\">Modular parts. Equipment is a separate visual — do not fuse.</text>\n\
    <text x=\"40\" y=\"{ay}\" font-size=\"13\" fill=\"#57534e\">Assembled</text>\n\
    <text x=\"640\" y=\"48\" font-size=\"13\" fill=\"#57534e\">Exploded parts · same scale</text>\n\
  </g>",
        ay = fmt_num(ASSEMBLED_ORIGIN[1] + 36.0),
    );

    write_assembled(&mut out, &pose, assembled);
    write_exploded(&mut out, &pose);
    let _ = writeln!(out, "</svg>");
    out
}

struct BindWorld {
    world: WorldPose,
}

impl BindWorld {
    fn xf(&self, bone: BoneIndex) -> BoneTransform {
        self.world.get(bone).expect("Humanoid v0 bone")
    }

    fn crown(&self) -> BoneTransform {
        self.xf(HEAD).compose(ANCHOR_CROWN)
    }

    fn chest(&self) -> BoneTransform {
        self.xf(TORSO).compose(ANCHOR_CHEST)
    }

    fn grip(&self, hand: BoneIndex) -> BoneTransform {
        self.xf(hand).compose(ANCHOR_GRIP)
    }

    fn foot_anchor(&self, foot: BoneIndex) -> BoneTransform {
        self.xf(foot).compose(ANCHOR_FOOT)
    }
}

fn bind_world() -> BindWorld {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).expect("Humanoid v0 bind evaluates");
    BindWorld { world }
}

fn write_assembled(out: &mut String, pose: &BindWorld, map: CanvasMap) {
    let _ = writeln!(out, "  <g id=\"assembled-side\">");
    let g0 = map.to_canvas([-0.55, 0.0]);
    let g1 = map.to_canvas([0.55, 0.0]);
    let _ = writeln!(
        out,
        "    <line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\" stroke=\"#a8a29e\" stroke-width=\"2\"/>",
        x1 = fmt_num(g0[0]),
        x2 = fmt_num(g1[0]),
        y = fmt_num(g0[1]),
    );

    write_body_stack(out, pose, |p| map.to_canvas(p));

    write_anchor_mark(out, map.to_canvas(pose.chest().translation), "Chest");
    write_anchor_mark(
        out,
        map.to_canvas(pose.grip(HAND_FRONT).translation),
        "Grip",
    );
    write_anchor_mark(
        out,
        map.to_canvas(pose.foot_anchor(FOOT_FRONT).translation),
        "Foot",
    );
    write_crown_target(out, map.to_canvas(pose.crown().translation));
    let _ = writeln!(out, "  </g>");
}

fn write_exploded(out: &mut String, pose: &BindWorld) {
    let _ = writeln!(out, "  <g id=\"exploded-parts\">");
    write_exploded_head(out, pose, [720.0, 200.0]);
    write_exploded_torso(out, pose, [1080.0, 240.0]);
    write_exploded_arm(out, pose, true, [720.0, 430.0]);
    write_exploded_arm(out, pose, false, [1080.0, 430.0]);
    write_exploded_hand(out, pose, true, [720.0, 600.0]);
    write_exploded_hand(out, pose, false, [1080.0, 600.0]);
    write_exploded_leg(out, pose, true, [720.0, 780.0]);
    write_exploded_leg(out, pose, false, [1080.0, 780.0]);
    write_exploded_foot(out, pose, true, [720.0, 940.0]);
    write_exploded_foot(out, pose, false, [1080.0, 940.0]);
    let _ = writeln!(out, "  </g>");
}

fn write_body_stack(out: &mut String, pose: &BindWorld, to_px: impl Fn([f32; 2]) -> [f32; 2]) {
    write_poly(out, &arm_polys(pose, false), FILL_BACK, &to_px);
    write_poly(out, &leg_polys(pose, false), FILL_BACK, &to_px);
    write_poly(
        out,
        &[corners_from_local(
            pose.xf(TORSO),
            &torso_far_local_corners(1.0),
        )],
        FILL_FAR,
        &to_px,
    );
    write_poly(
        out,
        &[corners_from_local(
            pose.xf(TORSO),
            &torso_local_corners(1.0),
        )],
        FILL_CORE,
        &to_px,
    );
    write_poly(out, &leg_polys(pose, true), FILL_FRONT, &to_px);
    write_poly(out, &[head_corners(pose.xf(HEAD))], FILL_HEAD, &to_px);
    write_poly(out, &arm_polys(pose, true), FILL_FRONT, &to_px);
}

fn arm_polys(pose: &BindWorld, front: bool) -> Vec<[[f32; 2]; 4]> {
    if front {
        vec![
            limb_corners(
                pose.xf(UPPER_ARM_FRONT),
                BIND_LOWER_ARM_FRONT.translation,
                ARM_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_ARM_FRONT),
                BIND_HAND_FRONT.translation,
                ARM_WIDTH,
            ),
            hand_corners(pose.xf(HAND_FRONT)),
        ]
    } else {
        vec![
            limb_corners(
                pose.xf(UPPER_ARM_BACK),
                BIND_LOWER_ARM_BACK.translation,
                ARM_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_ARM_BACK),
                BIND_HAND_BACK.translation,
                ARM_WIDTH,
            ),
            hand_corners(pose.xf(HAND_BACK)),
        ]
    }
}

fn arm_without_hand(pose: &BindWorld, front: bool) -> Vec<[[f32; 2]; 4]> {
    if front {
        vec![
            limb_corners(
                pose.xf(UPPER_ARM_FRONT),
                BIND_LOWER_ARM_FRONT.translation,
                ARM_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_ARM_FRONT),
                BIND_HAND_FRONT.translation,
                ARM_WIDTH,
            ),
        ]
    } else {
        vec![
            limb_corners(
                pose.xf(UPPER_ARM_BACK),
                BIND_LOWER_ARM_BACK.translation,
                ARM_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_ARM_BACK),
                BIND_HAND_BACK.translation,
                ARM_WIDTH,
            ),
        ]
    }
}

fn leg_polys(pose: &BindWorld, front: bool) -> Vec<[[f32; 2]; 4]> {
    if front {
        vec![
            limb_corners(
                pose.xf(UPPER_LEG_FRONT),
                BIND_LOWER_LEG_FRONT.translation,
                LEG_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_LEG_FRONT),
                BIND_FOOT_FRONT.translation,
                LEG_WIDTH,
            ),
            foot_corners(pose.xf(FOOT_FRONT)),
        ]
    } else {
        vec![
            limb_corners(
                pose.xf(UPPER_LEG_BACK),
                BIND_LOWER_LEG_BACK.translation,
                LEG_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_LEG_BACK),
                BIND_FOOT_BACK.translation,
                LEG_WIDTH,
            ),
            foot_corners(pose.xf(FOOT_BACK)),
        ]
    }
}

fn leg_without_foot(pose: &BindWorld, front: bool) -> Vec<[[f32; 2]; 4]> {
    if front {
        vec![
            limb_corners(
                pose.xf(UPPER_LEG_FRONT),
                BIND_LOWER_LEG_FRONT.translation,
                LEG_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_LEG_FRONT),
                BIND_FOOT_FRONT.translation,
                LEG_WIDTH,
            ),
        ]
    } else {
        vec![
            limb_corners(
                pose.xf(UPPER_LEG_BACK),
                BIND_LOWER_LEG_BACK.translation,
                LEG_WIDTH,
            ),
            limb_corners(
                pose.xf(LOWER_LEG_BACK),
                BIND_FOOT_BACK.translation,
                LEG_WIDTH,
            ),
        ]
    }
}

fn write_exploded_head(out: &mut String, pose: &BindWorld, cell: [f32; 2]) {
    let pivot = pose.xf(HEAD).translation;
    let to_px = |p| exploded_px(p, pivot, cell);
    write_poly(out, &[head_corners(pose.xf(HEAD))], FILL_HEAD, to_px);
    write_pivot(out, cell);
    write_crown_target(out, to_px(pose.crown().translation));
    write_part_label(out, cell, "Head");
}

fn write_exploded_torso(out: &mut String, pose: &BindWorld, cell: [f32; 2]) {
    let pivot = pose.xf(TORSO).translation;
    let to_px = |p| exploded_px(p, pivot, cell);
    write_poly(
        out,
        &[corners_from_local(
            pose.xf(TORSO),
            &torso_far_local_corners(1.0),
        )],
        FILL_FAR,
        to_px,
    );
    write_poly(
        out,
        &[corners_from_local(
            pose.xf(TORSO),
            &torso_local_corners(1.0),
        )],
        FILL_CORE,
        to_px,
    );
    write_pivot(out, cell);
    write_anchor_mark(out, to_px(pose.chest().translation), "Chest");
    write_part_label(out, cell, "Torso");
}

fn write_exploded_arm(out: &mut String, pose: &BindWorld, front: bool, cell: [f32; 2]) {
    let bone = if front {
        UPPER_ARM_FRONT
    } else {
        UPPER_ARM_BACK
    };
    let pivot = pose.xf(bone).translation;
    let fill = if front { FILL_FRONT } else { FILL_BACK };
    let to_px = |p| exploded_px(p, pivot, cell);
    write_poly(out, &arm_without_hand(pose, front), fill, to_px);
    write_pivot(out, cell);
    write_part_label(out, cell, if front { "ArmFront" } else { "ArmBack" });
}

fn write_exploded_hand(out: &mut String, pose: &BindWorld, front: bool, cell: [f32; 2]) {
    let bone = if front { HAND_FRONT } else { HAND_BACK };
    let pivot = pose.xf(bone).translation;
    let to_px = |p| exploded_px(p, pivot, cell);
    write_poly(out, &[hand_corners(pose.xf(bone))], FILL_HEAD, to_px);
    write_pivot(out, cell);
    write_anchor_mark(out, to_px(pose.grip(bone).translation), "Grip");
    write_part_label(out, cell, if front { "HandFront" } else { "HandBack" });
}

fn write_exploded_leg(out: &mut String, pose: &BindWorld, front: bool, cell: [f32; 2]) {
    let bone = if front {
        UPPER_LEG_FRONT
    } else {
        UPPER_LEG_BACK
    };
    let pivot = pose.xf(bone).translation;
    let fill = if front { FILL_FRONT } else { FILL_BACK };
    let to_px = |p| exploded_px(p, pivot, cell);
    write_poly(out, &leg_without_foot(pose, front), fill, to_px);
    write_pivot(out, cell);
    write_part_label(out, cell, if front { "LegFront" } else { "LegBack" });
}

fn write_exploded_foot(out: &mut String, pose: &BindWorld, front: bool, cell: [f32; 2]) {
    let bone = if front { FOOT_FRONT } else { FOOT_BACK };
    let pivot = pose.xf(bone).translation;
    let to_px = |p| exploded_px(p, pivot, cell);
    write_poly(out, &[foot_corners(pose.xf(bone))], FILL_CORE, to_px);
    write_pivot(out, cell);
    write_anchor_mark(out, to_px(pose.foot_anchor(bone).translation), "Foot");
    write_part_label(out, cell, if front { "FootFront" } else { "FootBack" });
}

fn exploded_px(world: [f32; 2], pivot: [f32; 2], cell: [f32; 2]) -> [f32; 2] {
    [
        cell[0] + (world[0] - pivot[0]) * PX_PER_WU,
        cell[1] - (world[1] - pivot[1]) * PX_PER_WU,
    ]
}

fn write_poly(
    out: &mut String,
    polys: &[[[f32; 2]; 4]],
    fill: &str,
    to_px: impl Fn([f32; 2]) -> [f32; 2],
) {
    for poly in polys {
        let pts: String = poly
            .iter()
            .map(|p| {
                let c = to_px(*p);
                format!("{},{}", fmt_num(c[0]), fmt_num(c[1]))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            out,
            "    <polygon points=\"{pts}\" fill=\"{fill}\" stroke=\"{STROKE}\" stroke-width=\"1.25\"/>"
        );
    }
}

fn write_pivot(out: &mut String, cell: [f32; 2]) {
    let _ = writeln!(
        out,
        "    <circle cx=\"{x}\" cy=\"{y}\" r=\"3.5\" fill=\"{STROKE}\"/>",
        x = fmt_num(cell[0]),
        y = fmt_num(cell[1]),
    );
}

fn write_part_label(out: &mut String, cell: [f32; 2], name: &str) {
    let _ = writeln!(
        out,
        "    <text x=\"{x}\" y=\"{y}\" font-family=\"Segoe UI,Arial,sans-serif\" font-size=\"12\" fill=\"#44403c\">{name}</text>",
        x = fmt_num(cell[0] + 14.0),
        y = fmt_num(cell[1] + 4.0),
    );
}

fn write_anchor_mark(out: &mut String, c: [f32; 2], name: &str) {
    let x = c[0];
    let y = c[1];
    let _ = writeln!(
        out,
        "    <polygon points=\"{x},{y1} {x2},{y} {x},{y2} {x1},{y}\" fill=\"{ANCHOR}\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Segoe UI,Arial,sans-serif\" font-size=\"11\" font-weight=\"700\" fill=\"{ANCHOR}\">{name}</text>",
        x = fmt_num(x),
        y = fmt_num(y),
        x1 = fmt_num(x - 6.0),
        x2 = fmt_num(x + 6.0),
        y1 = fmt_num(y - 6.0),
        y2 = fmt_num(y + 6.0),
        tx = fmt_num(x + 9.0),
        ty = fmt_num(y - 8.0),
    );
}

fn write_crown_target(out: &mut String, c: [f32; 2]) {
    let rx = 0.13 * PX_PER_WU * 0.5;
    let ry = 0.07 * PX_PER_WU * 0.55;
    let cy = c[1] - ry * 0.35;
    let _ = writeln!(
        out,
        "    <ellipse cx=\"{x}\" cy=\"{y}\" rx=\"{rx}\" ry=\"{ry}\" fill=\"none\" stroke=\"{ANCHOR}\" stroke-width=\"1.75\" stroke-dasharray=\"5 4\"/>",
        x = fmt_num(c[0]),
        y = fmt_num(cy),
        rx = fmt_num(rx),
        ry = fmt_num(ry),
    );
    write_anchor_mark(out, c, "Crown");
}

fn limb_corners(bone: BoneTransform, child_rest: [f32; 2], width: f32) -> [[f32; 2]; 4] {
    let length = child_rest[0].hypot(child_rest[1]);
    let rot = bone.rotation + child_rest[0].atan2(-child_rest[1]);
    let xf = BoneTransform {
        translation: bone.translation,
        rotation: rot,
    };
    corners_from_local(
        xf,
        &[
            [-width * 0.5, 0.0],
            [width * 0.5, 0.0],
            [width * 0.5, -length],
            [-width * 0.5, -length],
        ],
    )
}

fn hand_corners(bone: BoneTransform) -> [[f32; 2]; 4] {
    let w = HAND_SIZE[0];
    let h = HAND_SIZE[1];
    corners_from_local(
        bone,
        &[
            [-w * 0.5, 0.0],
            [w * 0.5, 0.0],
            [w * 0.5, -h],
            [-w * 0.5, -h],
        ],
    )
}

fn foot_corners(bone: BoneTransform) -> [[f32; 2]; 4] {
    let w = FOOT_SIZE[0];
    let h = FOOT_SIZE[1];
    corners_from_local(
        bone,
        &[[0.0, -h * 0.5], [w, -h * 0.5], [w, h * 0.5], [0.0, h * 0.5]],
    )
}

fn head_corners(bone: BoneTransform) -> [[f32; 2]; 4] {
    let w = HEAD_BASE[0] * HEAD_VISUAL_SCALE * HEAD_WIDTH_SCALE;
    let h = HEAD_BASE[1] * HEAD_VISUAL_SCALE;
    corners_from_local(
        bone,
        &[
            [-w * 0.38, 0.0],
            [w * 0.38, 0.0],
            [w * 0.56, h],
            [-w * 0.22, h],
        ],
    )
}

fn corners_from_local(world: BoneTransform, local: &[[f32; 2]; 4]) -> [[f32; 2]; 4] {
    [
        apply_local(world, local[0]),
        apply_local(world, local[1]),
        apply_local(world, local[2]),
        apply_local(world, local[3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring_template;
    use purgatory_skeleton::{ANCHOR_CROWN, HEAD, LocalPose, WorldPose, evaluate, humanoid_v0};

    #[test]
    fn svg_is_deterministic() {
        assert_eq!(render_svg(), render_svg());
    }

    #[test]
    fn uses_same_scale_as_technical_template() {
        assert_eq!(assembled_map().px_per_wu, authoring_template::PX_PER_WU);
        assert_eq!(CANVAS_W, 1600);
        assert_eq!(CANVAS_H, 1024);
    }

    #[test]
    fn assembled_and_exploded_groups_exist() {
        let svg = render_svg();
        assert!(svg.contains("id=\"assembled-side\""));
        assert!(svg.contains("id=\"exploded-parts\""));
        assert!(!svg.contains("fill=\"#ffffff\""));
        assert!(!svg.contains("slot:"));
        assert!(!svg.contains("world_x"));
        for name in [
            "Head",
            "Torso",
            "ArmFront",
            "ArmBack",
            "HandFront",
            "HandBack",
            "LegFront",
            "LegBack",
            "FootFront",
            "FootBack",
            "Crown",
            "Chest",
            "Grip",
            "Foot",
        ] {
            assert!(svg.contains(name), "missing {name}");
        }
    }

    #[test]
    fn crown_matches_skeleton_anchor() {
        let def = humanoid_v0();
        let local = LocalPose::from_bind(def);
        let mut world = WorldPose::new(def);
        evaluate(def, &local, &mut world).unwrap();
        let expected = world.get(HEAD).unwrap().compose(ANCHOR_CROWN).translation;
        assert_eq!(crown_world()[0], expected[0]);
        assert_eq!(crown_world()[1], expected[1]);
        let px = crown_assembled_canvas();
        assert_eq!(px, assembled_map().to_canvas(expected));
        let svg = render_svg();
        assert!(svg.contains(&format!("{},{}", fmt_num(px[0] - 6.0), fmt_num(px[1]))));
    }

    #[test]
    fn default_path_is_ai_modular_v1() {
        let path = default_output_path();
        let display = path.to_string_lossy();
        assert!(
            display.contains("HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg"),
            "{display}"
        );
    }

    #[test]
    fn does_not_replace_technical_template() {
        assert_ne!(
            default_output_path().file_name(),
            authoring_template::default_output_path().file_name()
        );
    }

    #[test]
    fn export_roundtrip_temp() {
        let dir = std::env::temp_dir().join("purgatory_ai_modular_reference_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("HUMANOID_V0_AI_MODULAR_REFERENCE_V1.svg");
        export_to(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), render_svg());
    }

    #[test]
    fn committed_reference_matches_generator() {
        let on_disk = std::fs::read_to_string(default_output_path()).unwrap_or_default();
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            render_svg().replace("\r\n", "\n"),
            "re-export with PURGATORY_WRITE_AUTHORING_TEMPLATE=1 cargo test -p purgatory-dev-hub --bin purgatory-dev-hub write_authoring_template_if_requested"
        );
    }

    #[test]
    fn head_height_uses_runtime_placeholder_span() {
        let w = HEAD_BASE[0] * HEAD_VISUAL_SCALE * HEAD_WIDTH_SCALE;
        let h = HEAD_BASE[1] * HEAD_VISUAL_SCALE;
        assert!((w - 0.308).abs() < 1e-6);
        assert!((h - 0.32).abs() < 1e-6);
        assert!(h > purgatory_skeleton::BIND_HEAD.translation[1]);
    }
}
