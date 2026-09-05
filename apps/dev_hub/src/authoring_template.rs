//! Post-8 Humanoid v0 master authoring template (Developer Hub only).
//!
//! Reads live bind / slot-rest / anchor / torso-placeholder contracts from
//! `purgatory-skeleton`. Joint coordinates are not copied into this module.
//! Output is a fixed-size transparent SVG for Photoshop. Not a runtime asset.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use purgatory_skeleton::{
    ANCHOR_CHEST, ANCHOR_CROWN, ANCHOR_FOOT, ANCHOR_GRIP, BoneIndex, BoneTransform, FOOT_BACK,
    FOOT_FRONT, HAND_BACK, HAND_FRONT, HEAD, HUMANOID_V0_BONE_LABELS, HUMANOID_V0_SLOT_LABELS,
    LOWER_ARM_BACK, LOWER_ARM_FRONT, LOWER_LEG_BACK, LOWER_LEG_FRONT, LocalPose, SlotIndex, TORSO,
    UPPER_ARM_BACK, UPPER_ARM_FRONT, UPPER_LEG_BACK, UPPER_LEG_FRONT, WorldPose, evaluate,
    humanoid_v0, slot_world, torso_local_corners,
};

/// Canvas width (px). Origin is the top-left of the SVG.
pub const CANVAS_W: u32 = 1024;
/// Canvas height (px).
pub const CANVAS_H: u32 = 1024;
/// Pixels per presentation-world unit. Integer multiple of the 8B 128 px
/// attachment-correction canvas (`AUTHORING_CANVAS_PX` in the client compose module).
pub const PX_PER_WU: f32 = 256.0;
/// Canvas X of world origin (character center / root).
pub const ORIGIN_X: f32 = 512.0;
/// Canvas Y of world origin (ground line). SVG Y grows downward.
pub const ORIGIN_Y: f32 = 768.0;
/// Extra world-space padding around evaluated joints / slots / anchors.
pub const SAFE_MARGIN_WU: f32 = 0.12;

const OUTPUT_REL: &str = "Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg";

/// Default repository path for the generated template.
#[must_use]
pub fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(OUTPUT_REL)
}

/// World (+X right, +Y up) → canvas pixels (origin top-left, +Y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasMap {
    pub origin: [f32; 2],
    pub px_per_wu: f32,
}

impl CanvasMap {
    pub const TECHNICAL: Self = Self {
        origin: [ORIGIN_X, ORIGIN_Y],
        px_per_wu: PX_PER_WU,
    };

    #[must_use]
    pub fn to_canvas(self, xy: [f32; 2]) -> [f32; 2] {
        [
            self.origin[0] + xy[0] * self.px_per_wu,
            self.origin[1] - xy[1] * self.px_per_wu,
        ]
    }
}

/// World (+X right, +Y up) → technical-template canvas pixels.
#[must_use]
pub fn world_to_canvas(xy: [f32; 2]) -> [f32; 2] {
    CanvasMap::TECHNICAL.to_canvas(xy)
}

/// Write the template. Parent directories are created as needed.
pub fn export_to(path: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("create dir: {err}"))?;
    }
    std::fs::write(path, render_svg()).map_err(|err| format!("write {}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

/// Deterministic SVG. Regenerates from current Humanoid v0 contracts.
#[must_use]
pub fn render_svg() -> String {
    let def = humanoid_v0();
    let local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    evaluate(def, &local, &mut world).expect("Humanoid v0 bind evaluates");

    let mut joints = Vec::with_capacity(def.bone_count());
    for i in 0..def.bone_count() {
        let bone = BoneIndex::from_u8(i as u8);
        let xf = world.get(bone).expect("bone in range");
        joints.push((bone, xf));
    }

    let mut slots = Vec::with_capacity(def.slot_count());
    for i in 0..def.slot_count() {
        let slot = SlotIndex::from_u8(i as u8);
        let xf = slot_world(def, &world, slot).expect("slot in range");
        slots.push((slot, xf));
    }

    let head = world.get(HEAD).expect("head");
    let torso = world.get(TORSO).expect("torso");
    let hand_front = world.get(HAND_FRONT).expect("hand_front");
    let hand_back = world.get(HAND_BACK).expect("hand_back");
    let foot_front = world.get(FOOT_FRONT).expect("foot_front");
    let foot_back = world.get(FOOT_BACK).expect("foot_back");

    let crown = head.compose(ANCHOR_CROWN);
    let chest = torso.compose(ANCHOR_CHEST);
    let grip_front = hand_front.compose(ANCHOR_GRIP);
    let grip_back = hand_back.compose(ANCHOR_GRIP);
    let foot_anchor_front = foot_front.compose(ANCHOR_FOOT);
    let foot_anchor_back = foot_back.compose(ANCHOR_FOOT);

    let torso_world: Vec<[f32; 2]> = torso_local_corners(1.0)
        .into_iter()
        .map(|p| apply_local(torso, p))
        .collect();

    let mut xs: Vec<f32> = Vec::new();
    let mut ys: Vec<f32> = Vec::new();
    for (_, xf) in &joints {
        xs.push(xf.translation[0]);
        ys.push(xf.translation[1]);
    }
    for (_, xf) in &slots {
        xs.push(xf.translation[0]);
        ys.push(xf.translation[1]);
    }
    for p in [
        crown.translation,
        chest.translation,
        grip_front.translation,
        grip_back.translation,
        foot_anchor_front.translation,
        foot_anchor_back.translation,
    ] {
        xs.push(p[0]);
        ys.push(p[1]);
    }
    for p in &torso_world {
        xs.push(p[0]);
        ys.push(p[1]);
    }
    let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min) - SAFE_MARGIN_WU;
    let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max) + SAFE_MARGIN_WU;
    let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min) - SAFE_MARGIN_WU;
    let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max) + SAFE_MARGIN_WU;

    let safe = [
        world_to_canvas([min_x, max_y]),
        world_to_canvas([max_x, max_y]),
        world_to_canvas([max_x, min_y]),
        world_to_canvas([min_x, min_y]),
    ];
    let safe_w = (max_x - min_x) * PX_PER_WU;
    let safe_h = (max_y - min_y) * PX_PER_WU;

    let mut out = String::with_capacity(12_000);
    let _ = writeln!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{CANVAS_W}\" height=\"{CANVAS_H}\" viewBox=\"0 0 {CANVAS_W} {CANVAS_H}\">\n\
  <title>PURGATORY Humanoid v0 authoring template</title>\n\
  <desc>Transparent Photoshop reference. World +X right, +Y up. Canvas origin top-left. \
1 wu = {px} px. Character origin (root / ground center) = ({ox}, {oy}). \
x = {ox} + world_x * {px}; y = {oy} - world_y * {px}. \
Bind pose is PresentationView::Side (RIGHT 3/4). Generated from purgatory-skeleton; do not hand-edit coordinates.</desc>",
        px = fmt_num(PX_PER_WU),
        ox = fmt_num(ORIGIN_X),
        oy = fmt_num(ORIGIN_Y),
    );

    let _ = writeln!(
        out,
        "  <g id=\"canvas-bounds\" fill=\"none\" stroke=\"#6b7280\" stroke-width=\"2\">\n\
    <rect x=\"0.5\" y=\"0.5\" width=\"{w}\" height=\"{h}\"/>\n\
  </g>",
        w = fmt_num(CANVAS_W as f32 - 1.0),
        h = fmt_num(CANVAS_H as f32 - 1.0),
    );

    let _ = writeln!(
        out,
        "  <g id=\"side-back-guides\" fill=\"none\">\n\
    <text x=\"24\" y=\"36\" font-family=\"Consolas,monospace\" font-size=\"16\" fill=\"#111827\">Humanoid v0 bind · PresentationView::Side (RIGHT 3/4)</text>\n\
    <text x=\"24\" y=\"56\" font-family=\"Consolas,monospace\" font-size=\"12\" fill=\"#4b5563\">Back view hides ArmFront/LegFront; remaining ArmBack/LegBack paint near. Not a second skeleton.</text>\n\
    <text x=\"24\" y=\"{left_y}\" font-family=\"Consolas,monospace\" font-size=\"14\" fill=\"#b45309\" transform=\"rotate(-90 24 {left_y})\">FRONT (near)</text>\n\
    <text x=\"{right_x}\" y=\"{right_y}\" font-family=\"Consolas,monospace\" font-size=\"14\" fill=\"#1d4ed8\" transform=\"rotate(90 {right_x} {right_y})\">BACK (far)</text>\n\
  </g>",
        left_y = fmt_num(ORIGIN_Y - 80.0),
        right_x = fmt_num(CANVAS_W as f32 - 24.0),
        right_y = fmt_num(ORIGIN_Y - 80.0),
    );

    let _ = writeln!(
        out,
        "  <g id=\"safe-bounds\" fill=\"none\" stroke=\"#38bdf8\" stroke-width=\"1.5\" stroke-dasharray=\"8 6\">\n\
    <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Consolas,monospace\" font-size=\"11\" fill=\"#0369a1\">safe bounds (+{margin} wu)</text>\n\
  </g>",
        x = fmt_num(safe[0][0]),
        y = fmt_num(safe[0][1]),
        w = fmt_num(safe_w),
        h = fmt_num(safe_h),
        tx = fmt_num(safe[0][0] + 6.0),
        ty = fmt_num(safe[0][1] - 6.0),
        margin = fmt_num(SAFE_MARGIN_WU),
    );

    let ground = world_to_canvas([-2.0, 0.0]);
    let ground_end = world_to_canvas([2.0, 0.0]);
    let _ = writeln!(
        out,
        "  <g id=\"ground-line\" fill=\"none\" stroke=\"#059669\" stroke-width=\"2\">\n\
    <line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Consolas,monospace\" font-size=\"12\" fill=\"#047857\">ground y=0</text>\n\
  </g>",
        x1 = fmt_num(ground[0]),
        x2 = fmt_num(ground_end[0]),
        y = fmt_num(ground[1]),
        tx = fmt_num(ORIGIN_X + 24.0),
        ty = fmt_num(ground[1] - 8.0),
    );

    let origin = world_to_canvas([0.0, 0.0]);
    let _ = writeln!(
        out,
        "  <g id=\"character-center\" stroke=\"#dc2626\" stroke-width=\"1.5\" fill=\"none\">\n\
    <line x1=\"{x}\" y1=\"{y1}\" x2=\"{x}\" y2=\"{y2}\"/>\n\
    <line x1=\"{x1}\" y1=\"{y}\" x2=\"{x2}\" y2=\"{y}\"/>\n\
    <circle cx=\"{x}\" cy=\"{y}\" r=\"5\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Consolas,monospace\" font-size=\"12\" fill=\"#b91c1c\">center / root (0,0)</text>\n\
  </g>",
        x = fmt_num(origin[0]),
        y = fmt_num(origin[1]),
        y1 = fmt_num(origin[1] - 18.0),
        y2 = fmt_num(origin[1] + 18.0),
        x1 = fmt_num(origin[0] - 18.0),
        x2 = fmt_num(origin[0] + 18.0),
        tx = fmt_num(origin[0] + 12.0),
        ty = fmt_num(origin[1] + 28.0),
    );

    let poly: String = torso_world
        .iter()
        .map(|p| {
            let c = world_to_canvas(*p);
            format!("{},{}", fmt_num(c[0]), fmt_num(c[1]))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(
        out,
        "  <g id=\"bind-body\">\n\
    <polygon points=\"{poly}\" fill=\"#d6d3d1\" fill-opacity=\"0.55\" stroke=\"#44403c\" stroke-width=\"1.5\"/>\n\
  </g>"
    );

    let _ = writeln!(
        out,
        "  <g id=\"skeleton\" fill=\"none\" stroke-linecap=\"round\">"
    );
    for (bone, xf) in &joints {
        let Some(parent) = def.parent(*bone) else {
            continue;
        };
        let parent_xf = world.get(parent).expect("parent in range");
        let a = world_to_canvas(parent_xf.translation);
        let b = world_to_canvas(xf.translation);
        let (stroke, dash) = limb_stroke(*bone);
        let dash_attr = if dash.is_empty() {
            String::new()
        } else {
            format!(" stroke-dasharray=\"{dash}\"")
        };
        let _ = writeln!(
            out,
            "    <line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"{stroke}\" stroke-width=\"3\"{dash}/>",
            x1 = fmt_num(a[0]),
            y1 = fmt_num(a[1]),
            x2 = fmt_num(b[0]),
            y2 = fmt_num(b[1]),
            dash = dash_attr,
        );
    }
    let _ = writeln!(out, "  </g>");

    let _ = writeln!(out, "  <g id=\"bone-pivots\">");
    for (bone, xf) in &joints {
        let c = world_to_canvas(xf.translation);
        let label = HUMANOID_V0_BONE_LABELS[bone.as_usize()];
        let _ = writeln!(
            out,
            "    <circle cx=\"{x}\" cy=\"{y}\" r=\"4\" fill=\"#111827\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Consolas,monospace\" font-size=\"10\" fill=\"#111827\">{label}</text>",
            x = fmt_num(c[0]),
            y = fmt_num(c[1]),
            tx = fmt_num(c[0] + 7.0),
            ty = fmt_num(c[1] - 7.0),
        );
    }
    let _ = writeln!(out, "  </g>");

    let _ = writeln!(out, "  <g id=\"anchors\">");
    write_anchor(&mut out, "CROWN", crown.translation, "#c2410c");
    write_anchor(&mut out, "CHEST", chest.translation, "#c2410c");
    write_anchor(&mut out, "GRIP", grip_front.translation, "#c2410c");
    write_anchor(&mut out, "GRIP", grip_back.translation, "#c2410c");
    write_anchor(&mut out, "FOOT", foot_anchor_front.translation, "#c2410c");
    write_anchor(&mut out, "FOOT", foot_anchor_back.translation, "#c2410c");
    let _ = writeln!(out, "  </g>");

    let _ = writeln!(out, "  <g id=\"slots\">");
    for (slot, xf) in &slots {
        let c = world_to_canvas(xf.translation);
        let label = HUMANOID_V0_SLOT_LABELS[slot.as_usize()];
        let _ = writeln!(
            out,
            "    <rect x=\"{x}\" y=\"{y}\" width=\"7\" height=\"7\" fill=\"#2563eb\" fill-opacity=\"0.85\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Consolas,monospace\" font-size=\"9\" fill=\"#1e40af\">slot:{label}</text>",
            x = fmt_num(c[0] - 3.5),
            y = fmt_num(c[1] - 3.5),
            tx = fmt_num(c[0] + 8.0),
            ty = fmt_num(c[1] + 14.0),
        );
    }
    let _ = writeln!(out, "  </g>");

    let _ = writeln!(
        out,
        "  <g id=\"mapping\" font-family=\"Consolas,monospace\" font-size=\"11\" fill=\"#374151\">\n\
    <text x=\"24\" y=\"992\">canvas {CANVAS_W}×{CANVAS_H} px · origin top-left · 1 wu = {px} px · origin px ({ox}, {oy}) = world (0,0)</text>\n\
    <text x=\"24\" y=\"1008\">x = {ox} + world_x × {px} · y = {oy} − world_y × {px} · 8B attachment canvas 128 px = 1 local wu (not this sheet)</text>\n\
  </g>
</svg>",
        px = fmt_num(PX_PER_WU),
        ox = fmt_num(ORIGIN_X),
        oy = fmt_num(ORIGIN_Y),
    );

    out
}

pub(crate) fn apply_local(world: BoneTransform, local: [f32; 2]) -> [f32; 2] {
    world
        .compose(BoneTransform::from_translation_rotation(local, 0.0))
        .translation
}

fn limb_stroke(bone: BoneIndex) -> (&'static str, &'static str) {
    const FRONT: [BoneIndex; 6] = [
        UPPER_ARM_FRONT,
        LOWER_ARM_FRONT,
        HAND_FRONT,
        UPPER_LEG_FRONT,
        LOWER_LEG_FRONT,
        FOOT_FRONT,
    ];
    const BACK: [BoneIndex; 6] = [
        UPPER_ARM_BACK,
        LOWER_ARM_BACK,
        HAND_BACK,
        UPPER_LEG_BACK,
        LOWER_LEG_BACK,
        FOOT_BACK,
    ];
    if FRONT.contains(&bone) {
        ("#b45309", "")
    } else if BACK.contains(&bone) {
        ("#1d4ed8", "5 4")
    } else {
        ("#44403c", "")
    }
}

fn write_anchor(out: &mut String, name: &str, world: [f32; 2], color: &str) {
    let c = world_to_canvas(world);
    let x = c[0];
    let y = c[1];
    let _ = writeln!(
        out,
        "    <polygon points=\"{x},{y1} {x2},{y} {x},{y2} {x1},{y}\" fill=\"{color}\" fill-opacity=\"0.9\"/>\n\
    <text x=\"{tx}\" y=\"{ty}\" font-family=\"Consolas,monospace\" font-size=\"11\" font-weight=\"700\" fill=\"{color}\">{name}</text>",
        x = fmt_num(x),
        y = fmt_num(y),
        x1 = fmt_num(x - 7.0),
        x2 = fmt_num(x + 7.0),
        y1 = fmt_num(y - 7.0),
        y2 = fmt_num(y + 7.0),
        tx = fmt_num(x + 10.0),
        ty = fmt_num(y - 10.0),
    );
}

pub(crate) fn fmt_num(v: f32) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.abs() < 0.005 {
        "0.00".to_string()
    } else {
        format!("{r:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_skeleton::{ANCHOR_CROWN, HEAD, LocalPose, WorldPose, evaluate, humanoid_v0};

    #[test]
    fn svg_is_deterministic() {
        assert_eq!(render_svg(), render_svg());
    }

    #[test]
    fn canvas_is_fixed_size_transparent_svg() {
        let svg = render_svg();
        assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(svg.contains(&format!("width=\"{CANVAS_W}\"")));
        assert!(svg.contains(&format!("height=\"{CANVAS_H}\"")));
        assert!(svg.contains("viewBox=\"0 0 1024 1024\""));
        assert!(!svg.contains("fill=\"#ffffff\""));
        assert!(svg.contains("id=\"canvas-bounds\""));
        assert!(svg.contains("id=\"character-center\""));
        assert!(svg.contains("id=\"ground-line\""));
        assert!(svg.contains("id=\"bind-body\""));
        assert!(svg.contains("id=\"skeleton\""));
        assert!(svg.contains("id=\"bone-pivots\""));
        assert!(svg.contains("id=\"anchors\""));
        assert!(svg.contains("id=\"slots\""));
        assert!(svg.contains("id=\"side-back-guides\""));
        assert!(svg.contains("id=\"safe-bounds\""));
    }

    #[test]
    fn world_origin_maps_to_documented_pixel() {
        assert_eq!(world_to_canvas([0.0, 0.0]), [ORIGIN_X, ORIGIN_Y]);
        assert_eq!(
            world_to_canvas([1.0, 0.0]),
            [ORIGIN_X + PX_PER_WU, ORIGIN_Y]
        );
        assert_eq!(
            world_to_canvas([0.0, 1.0]),
            [ORIGIN_X, ORIGIN_Y - PX_PER_WU]
        );
    }

    #[test]
    fn px_per_wu_is_integer_multiple_of_attachment_canvas() {
        assert!((PX_PER_WU / 128.0 - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn crown_pixel_matches_evaluated_anchor() {
        let def = humanoid_v0();
        let local = LocalPose::from_bind(def);
        let mut world = WorldPose::new(def);
        evaluate(def, &local, &mut world).unwrap();
        let crown = world.get(HEAD).unwrap().compose(ANCHOR_CROWN);
        let px = world_to_canvas(crown.translation);
        let svg = render_svg();
        assert!(svg.contains(&format!(">{label}</text>", label = "CROWN")));
        assert!(
            svg.contains(&format!("cx=\"{}\"", fmt_num(px[0])))
                || svg.contains(&format!(
                    "points=\"{},{}",
                    fmt_num(px[0]),
                    fmt_num(px[1] - 7.0)
                ))
        );
    }

    #[test]
    fn required_labels_are_present() {
        let svg = render_svg();
        for label in [
            "CROWN",
            "CHEST",
            "GRIP",
            "FOOT",
            "FRONT (near)",
            "BACK (far)",
        ] {
            assert!(svg.contains(label), "missing {label}");
        }
        for slot in HUMANOID_V0_SLOT_LABELS {
            assert!(svg.contains(&format!("slot:{slot}")), "missing slot {slot}");
        }
        for bone in HUMANOID_V0_BONE_LABELS {
            assert!(svg.contains(bone), "missing bone {bone}");
        }
    }

    #[test]
    fn default_path_is_graphic_character_template() {
        let path = default_output_path();
        let display = path.to_string_lossy();
        assert!(
            display.contains("Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg")
                || display.contains("Graphic\\character\\HUMANOID_V0_AUTHORING_TEMPLATE.svg"),
            "{display}"
        );
    }

    #[test]
    fn export_roundtrip_temp() {
        let dir = std::env::temp_dir().join("purgatory_authoring_template_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("HUMANOID_V0_AUTHORING_TEMPLATE.svg");
        export_to(&path).unwrap();
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, render_svg());
    }

    #[test]
    fn write_authoring_template_if_requested() {
        if std::env::var("PURGATORY_WRITE_AUTHORING_TEMPLATE").as_deref() != Ok("1") {
            return;
        }
        export_to(&default_output_path()).expect("write committed template");
        crate::ai_modular_reference::export_to(&crate::ai_modular_reference::default_output_path())
            .expect("write committed AI modular reference");
    }

    #[test]
    fn committed_template_matches_generator() {
        let path = default_output_path();
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            on_disk.replace("\r\n", "\n"),
            render_svg().replace("\r\n", "\n"),
            "re-export with PURGATORY_WRITE_AUTHORING_TEMPLATE=1 cargo test -p purgatory-dev-hub --bin purgatory-dev-hub write_authoring_template_if_requested"
        );
    }
}
