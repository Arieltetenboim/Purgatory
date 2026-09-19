//! Optional sprite-sheet presentation for NPCs.
//!
//! This module owns only presentation metadata and playback. NPC gameplay
//! state remains in simulation, while the renderer consumes the resolved
//! frame and the existing generic `AssetRuntime` texture identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::asset_runtime::AssetRuntime;
use crate::character_presentation::PresentationActivity;
use crate::renderer::{DrawQuad, SpriteTextureId};

const ACCEPT_MANIFEST: &[u8] = include_bytes!("../../../Graphic/ui/animation/accept.json");
const ACCEPT_TEXTURE: &[u8] = include_bytes!("../../../Graphic/ui/animation/accept.png");
const TURN_MANIFEST: &[u8] = include_bytes!("../../../Graphic/ui/animation/turn.json");
const TURN_TEXTURE: &[u8] = include_bytes!("../../../Graphic/ui/animation/turn.png");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpritePlaybackMode {
    Loop,
    Once,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpriteAnimationClip {
    frames: Vec<u16>,
    frame_seconds: f32,
    mode: SpritePlaybackMode,
}

impl SpriteAnimationClip {
    fn new(frames: Vec<u16>, frame_seconds: f32, mode: SpritePlaybackMode) -> Self {
        Self {
            frames,
            frame_seconds,
            mode,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn frames(&self) -> &[u16] {
        &self.frames
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn mode(&self) -> SpritePlaybackMode {
        self.mode
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SpriteAnimationPlayer {
    clip_name: &'static str,
    frame_index: usize,
    elapsed: f32,
    finished: bool,
}

impl SpriteAnimationPlayer {
    pub(crate) fn new() -> Self {
        Self {
            clip_name: "idle",
            frame_index: 0,
            elapsed: 0.0,
            finished: false,
        }
    }

    pub(crate) fn set_clip(&mut self, name: &'static str) {
        if self.clip_name != name {
            self.clip_name = name;
            self.frame_index = 0;
            self.elapsed = 0.0;
            self.finished = false;
        }
    }

    pub(crate) fn advance(&mut self, clip: &SpriteAnimationClip, dt: f32) {
        if self.finished || clip.frames.is_empty() || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.elapsed += dt;
        while self.elapsed >= clip.frame_seconds {
            self.elapsed -= clip.frame_seconds;
            if self.frame_index + 1 < clip.frames.len() {
                self.frame_index += 1;
            } else if clip.mode == SpritePlaybackMode::Loop {
                self.frame_index = 0;
            } else {
                self.frame_index = clip.frames.len() - 1;
                self.finished = true;
                self.elapsed = 0.0;
                break;
            }
        }
    }

    pub(crate) fn frame(&self, clip: &SpriteAnimationClip) -> Option<u16> {
        clip.frames.get(self.frame_index).copied()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn finished(&self) -> bool {
        self.finished
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SpriteSheet {
    texture: SpriteTextureId,
    width: u32,
    height: u32,
    frame_width: u32,
    frame_height: u32,
    world_size: [f32; 2],
    authored_facing_left: bool,
    frames: Vec<[[f32; 2]; 4]>,
    clips: HashMap<&'static str, SpriteAnimationClip>,
}

#[derive(Clone, Debug)]
pub(crate) struct OverheadSheet {
    texture: SpriteTextureId,
    frames: Vec<OverheadFrame>,
    frame_seconds: f32,
}

#[derive(Clone, Copy, Debug)]
struct OverheadFrame {
    uv: [[f32; 2]; 4],
    visual_bounds: [u32; 4],
}

impl OverheadSheet {
    pub(crate) fn accept(assets: &mut AssetRuntime) -> Result<Self, String> {
        Self::load(
            assets,
            ACCEPT_MANIFEST,
            ACCEPT_TEXTURE,
            "ui.overhead.accept",
        )
    }

    pub(crate) fn turn(assets: &mut AssetRuntime) -> Result<Self, String> {
        Self::load(assets, TURN_MANIFEST, TURN_TEXTURE, "ui.overhead.turn")
    }

    fn load(
        assets: &mut AssetRuntime,
        manifest_bytes: &[u8],
        texture_bytes: &[u8],
        expected_id: &str,
    ) -> Result<Self, String> {
        let manifest: RawOverheadManifest =
            serde_json::from_slice(manifest_bytes).map_err(|error| error.to_string())?;
        if manifest.schema_version != 1
            || manifest.kind != "purgatory_sprite_animation"
            || manifest.id != expected_id
            || manifest.frame_order.len() != manifest.frames.len()
            || manifest.frame_size_px != [192, 1024]
            || !manifest.looped
            || manifest.registration != "bottom_center"
            || manifest
                .frame_order
                .iter()
                .copied()
                .enumerate()
                .any(|(index, frame)| frame != index as u16)
        {
            return Err(format!(
                "unsupported overhead animation manifest {expected_id}"
            ));
        }
        let texture = assets.register_png(&manifest.id, texture_bytes)?;
        let resource = assets
            .resource(texture)
            .ok_or_else(|| format!("{expected_id} texture was not registered"))?;
        let width = resource.image.width();
        let height = resource.image.height();
        if [width, height] != manifest.dimensions_px {
            return Err(format!(
                "{expected_id} dimensions do not match decoded texture"
            ));
        }
        let frames = manifest
            .frames
            .into_iter()
            .map(|frame| {
                let [x, y, w, h] = [frame.x, frame.y, frame.width, frame.height];
                if w == 0
                    || h == 0
                    || x.checked_add(w).is_none_or(|right| right > width)
                    || y.checked_add(h).is_none_or(|bottom| bottom > height)
                    || frame.visual_bounds[2] == 0
                    || frame.visual_bounds[3] == 0
                    || frame.visual_bounds[0] + frame.visual_bounds[2] > w
                    || frame.visual_bounds[1] + frame.visual_bounds[3] > h
                {
                    return Err(format!("{expected_id} contains an invalid frame"));
                }
                let [vx, vy, vw, vh] = frame.visual_bounds;
                Ok(OverheadFrame {
                    uv: [
                        [
                            (x + vx) as f32 / width as f32,
                            (y + vy + vh) as f32 / height as f32,
                        ],
                        [
                            (x + vx + vw) as f32 / width as f32,
                            (y + vy + vh) as f32 / height as f32,
                        ],
                        [
                            (x + vx + vw) as f32 / width as f32,
                            (y + vy) as f32 / height as f32,
                        ],
                        [
                            (x + vx) as f32 / width as f32,
                            (y + vy) as f32 / height as f32,
                        ],
                    ],
                    visual_bounds: [vx, vy, vw, vh],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            texture,
            frames,
            frame_seconds: manifest.frame_seconds,
        })
    }

    pub(crate) fn advance(&self, player: &mut SpriteAnimationPlayer, dt: f32) {
        let clip = SpriteAnimationClip::new(
            (0..self.frames.len()).map(|index| index as u16).collect(),
            self.frame_seconds,
            SpritePlaybackMode::Loop,
        );
        player.advance(&clip, dt);
    }

    pub(crate) fn quad(&self, player: &SpriteAnimationPlayer, position: [f32; 2]) -> DrawQuad {
        let frame = self.frames[player.frame_index % self.frames.len()];
        let scale = 1.0 / 256.0;
        let width = frame.visual_bounds[2] as f32 * scale;
        let height = frame.visual_bounds[3] as f32 * scale;
        DrawQuad::textured_sprite(
            self.texture,
            [position[0], position[1] + height * 0.5],
            [
                [-width * 0.5, -height * 0.5],
                [width * 0.5, -height * 0.5],
                [width * 0.5, height * 0.5],
                [-width * 0.5, height * 0.5],
            ],
            frame.uv,
            0.0,
        )
    }
}

fn workspace_root() -> PathBuf {
    std::env::current_dir()
        .ok()
        .filter(|path| path.join("Graphic").join("creature").is_dir())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn find_sprite_manifest(sprite_id: &str) -> Result<(PathBuf, RawManifest), String> {
    let root = workspace_root().join("Graphic").join("creature");
    let entries =
        std::fs::read_dir(&root).map_err(|error| format!("read {}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&manifest_path)
            .map_err(|error| format!("read {}: {error}", manifest_path.display()))?;
        let header: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
        if header.get("id").and_then(serde_json::Value::as_str) != Some(sprite_id) {
            continue;
        }
        let manifest: RawManifest = serde_json::from_value(header)
            .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
        return Ok((manifest_path, manifest));
    }
    Err(format!(
        "sprite manifest '{sprite_id}' not found under {}",
        root.display()
    ))
}

impl SpriteSheet {
    pub(crate) fn from_sprite_id(
        assets: &mut AssetRuntime,
        sprite_id: &str,
    ) -> Result<Self, String> {
        let (manifest_path, manifest) = find_sprite_manifest(sprite_id)?;
        if manifest.schema_version != 1
            || manifest.kind != "purgatory_sprite_animation"
            || !matches!(manifest.authored_facing.as_str(), "left" | "right")
            || manifest.frame_size_px[0] == 0
            || manifest.frame_size_px[1] == 0
            || !manifest.world_size[0].is_finite()
            || !manifest.world_size[1].is_finite()
            || manifest.world_size[0] <= 0.0
            || manifest.world_size[1] <= 0.0
            || !manifest.frame_seconds.is_finite()
            || manifest.frame_seconds <= 0.0
        {
            return Err(format!(
                "unsupported sprite manifest {}",
                manifest_path.display()
            ));
        }
        let atlas_name = Path::new(&manifest.atlas);
        if atlas_name.components().count() != 1 {
            return Err(format!(
                "sprite atlas must be a file name: {}",
                manifest.atlas
            ));
        }
        let atlas_path = manifest_path
            .parent()
            .ok_or_else(|| "sprite manifest has no parent directory".to_owned())?
            .join(atlas_name);
        let atlas_bytes = std::fs::read(&atlas_path)
            .map_err(|error| format!("read {}: {error}", atlas_path.display()))?;
        let texture_key = format!("{}.atlas", manifest.id);
        let texture = assets.register_png(&texture_key, &atlas_bytes)?;
        let resource = assets
            .resource(texture)
            .ok_or_else(|| format!("sprite texture '{}' was not registered", manifest.id))?;
        let (width, height) = (resource.image.width(), resource.image.height());
        let [frame_width, frame_height] = manifest.frame_size_px;
        if width % frame_width != 0 || height % frame_height != 0 {
            return Err(format!(
                "{} atlas dimensions {}x{} are not divisible by frame size {}x{}",
                manifest.id, width, height, frame_width, frame_height
            ));
        }
        let columns = width / frame_width;
        let rows = height / frame_height;
        let frames: Vec<[[f32; 2]; 4]> = (0..columns * rows)
            .map(|index| {
                let x = index % columns * frame_width;
                let y = index / columns * frame_height;
                [
                    [
                        x as f32 / width as f32,
                        (y + frame_height) as f32 / height as f32,
                    ],
                    [
                        (x + frame_width) as f32 / width as f32,
                        (y + frame_height) as f32 / height as f32,
                    ],
                    [
                        (x + frame_width) as f32 / width as f32,
                        y as f32 / height as f32,
                    ],
                    [x as f32 / width as f32, y as f32 / height as f32],
                ]
            })
            .collect();

        let mut clips = HashMap::new();
        for (name, raw) in manifest.clips {
            let name = match name.as_str() {
                "idle" => "idle",
                "move" => "move",
                "attack" => "attack",
                _ => return Err(format!("unsupported sprite clip {name}")),
            };
            let mode = if raw.looped {
                SpritePlaybackMode::Loop
            } else {
                SpritePlaybackMode::Once
            };
            let clip = SpriteAnimationClip::new(raw.frames, manifest.frame_seconds, mode);
            if clip
                .frames
                .iter()
                .any(|frame| usize::from(*frame) >= frames.len())
            {
                return Err(format!(
                    "{} clip {name} references an invalid frame",
                    manifest.id
                ));
            }
            clips.insert(name, clip);
        }
        for required in ["idle", "move", "attack"] {
            if !clips.contains_key(required) {
                return Err(format!("{} manifest is missing {required}", manifest.id));
            }
        }

        Ok(Self {
            texture,
            width,
            height,
            frame_width,
            frame_height,
            world_size: manifest.world_size,
            authored_facing_left: manifest.authored_facing == "left",
            frames,
            clips,
        })
    }

    pub(crate) fn clip(&self, name: &'static str) -> &SpriteAnimationClip {
        &self.clips[name]
    }

    pub(crate) fn frame_uv(&self, frame: u16) -> [[f32; 2]; 4] {
        self.frames[usize::from(frame)]
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn dimensions_px(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn cell_size_px(&self) -> (u32, u32) {
        (self.frame_width, self.frame_height)
    }

    pub(crate) fn quad(
        &self,
        position: [f32; 2],
        frame: u16,
        flash: bool,
        facing_left: bool,
    ) -> DrawQuad {
        let half = [self.world_size[0] * 0.5, self.world_size[1] * 0.5];
        let mut quad = DrawQuad::textured_sprite(
            self.texture,
            position,
            [
                [-half[0], -half[1]],
                [half[0], -half[1]],
                [half[0], half[1]],
                [-half[0], half[1]],
            ],
            self.frame_uv(frame),
            0.0,
        );
        if flash {
            quad.color = [1.0, 0.72, 0.16, 1.0];
        }
        if facing_left != self.authored_facing_left {
            quad = quad.mirror_x_about(position);
        }
        quad
    }
}
#[derive(Deserialize)]
struct RawManifest {
    frame_size_px: [u32; 2],
    world_size: [f32; 2],
    frame_seconds: f32,
    authored_facing: String,
    clips: HashMap<String, RawClip>,
    atlas: String,
    schema_version: u32,
    kind: String,
    id: String,
}

#[derive(Deserialize)]
struct RawClip {
    frames: Vec<u16>,
    #[serde(rename = "loop")]
    looped: bool,
}

#[derive(Deserialize)]
struct RawOverheadManifest {
    schema_version: u32,
    kind: String,
    id: String,
    dimensions_px: [u32; 2],
    frame_size_px: [u32; 2],
    frames: Vec<RawOverheadFrame>,
    frame_order: Vec<u16>,
    frame_seconds: f32,
    #[serde(rename = "loop")]
    looped: bool,
    registration: String,
}

#[derive(Deserialize)]
struct RawOverheadFrame {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    visual_bounds: [u32; 4],
}

#[must_use]
pub(crate) fn base_activity(
    velocity_x: f32,
    one_shot: Option<PresentationActivity>,
) -> PresentationActivity {
    match one_shot {
        Some(PresentationActivity::Attack) => PresentationActivity::Attack,
        Some(PresentationActivity::Hurt) => PresentationActivity::Hurt,
        _ if velocity_x.abs() > 0.01 => PresentationActivity::Move,
        _ => PresentationActivity::Idle,
    }
}

#[must_use]
pub(crate) fn clip_name(activity: PresentationActivity) -> &'static str {
    match activity {
        PresentationActivity::Attack => "attack",
        PresentationActivity::Move => "move",
        _ => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> SpriteSheet {
        let mut assets = AssetRuntime::new();
        SpriteSheet::from_sprite_id(&mut assets, "creature.red_slime").unwrap()
    }

    #[test]
    fn sheet_resolves_fixed_four_by_four_cells() {
        let sheet = sheet();
        assert_eq!(sheet.dimensions_px(), (256, 256));
        assert_eq!(sheet.cell_size_px(), (64, 64));
        assert_eq!(sheet.frames.len(), 16);
        assert_eq!(
            sheet.frame_uv(0),
            [[0.0, 0.25], [0.25, 0.25], [0.25, 0.0], [0.0, 0.0]]
        );
    }

    #[test]
    fn idle_selects_row_three_and_loops() {
        let sheet = sheet();
        let clip = sheet.clip("idle");
        assert_eq!(clip.frames(), &[8, 9, 10, 11]);
        assert_eq!(clip.mode(), SpritePlaybackMode::Loop);
        let mut player = SpriteAnimationPlayer::new();
        player.advance(clip, 0.10 * 4.0);
        assert_eq!(player.frame(clip), Some(8));
        assert!(!player.finished());
    }

    #[test]
    fn move_selects_rows_one_and_two_and_loops() {
        let sheet = sheet();
        let clip = sheet.clip("move");
        assert_eq!(clip.frames(), &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(clip.mode(), SpritePlaybackMode::Loop);
    }

    #[test]
    fn attack_selects_row_four_and_is_one_shot() {
        let sheet = sheet();
        let clip = sheet.clip("attack");
        assert_eq!(clip.frames(), &[12, 13, 14, 15]);
        assert_eq!(clip.mode(), SpritePlaybackMode::Once);
        let mut player = SpriteAnimationPlayer::new();
        player.set_clip("attack");
        let mut drawn = Vec::new();
        for _ in clip.frames() {
            drawn.push(player.frame(clip).unwrap());
            player.advance(clip, 0.10);
        }
        assert_eq!(drawn, clip.frames());
        assert_eq!(player.frame(clip), Some(15));
        assert!(player.finished());
    }

    #[test]
    fn attack_is_selected_only_by_presentation_activity() {
        assert_eq!(
            clip_name(base_activity(0.0, Some(PresentationActivity::Attack))),
            "attack"
        );
        assert_eq!(
            base_activity(0.0, Some(PresentationActivity::Attack)),
            PresentationActivity::Attack
        );
        assert_eq!(clip_name(base_activity(1.0, None)), "move");
        assert_eq!(clip_name(base_activity(0.0, None)), "idle");
    }

    #[test]
    fn left_mirror_reuses_same_texture_and_uvs() {
        let sheet = sheet();
        let right = sheet.quad([2.0, 3.0], 8, false, false);
        let left = sheet.quad([2.0, 3.0], 8, false, true);
        assert_eq!(right.sprite_texture_id(), left.sprite_texture_id());
        assert_eq!(
            right.uvs(),
            [[0.0, 0.75], [0.25, 0.75], [0.25, 0.5], [0.0, 0.5]]
        );
        assert_eq!(
            left.uvs(),
            [[0.25, 0.75], [0.0, 0.75], [0.0, 0.5], [0.25, 0.5]]
        );
    }

    #[test]
    fn hurt_flash_keeps_the_active_clip_frame() {
        let sheet = sheet();
        let clip = sheet.clip("move");
        let mut player = SpriteAnimationPlayer::new();
        player.set_clip("move");
        player.advance(clip, 0.10);
        let frame = player.frame(clip).unwrap();
        let quad = sheet.quad([0.0, 0.0], frame, true, false);
        assert_eq!(frame, 1);
        assert_eq!(quad.sprite_texture_id(), Some(sheet.texture));
        assert_eq!(quad.color, [1.0, 0.72, 0.16, 1.0]);
    }

    #[test]
    fn manifest_metadata_is_validated_by_the_resolved_sheet() {
        let bytes = std::fs::read(
            workspace_root()
                .join("Graphic")
                .join("creature")
                .join("redslime")
                .join("manifest.json"),
        )
        .unwrap();
        let raw: RawManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw.schema_version, 1);
        assert_eq!(raw.kind, "purgatory_sprite_animation");
        assert_eq!(raw.id, "creature.red_slime");
        assert_eq!(raw.atlas, "redslime.png");
        assert_eq!(raw.authored_facing, "right");
    }

    #[test]
    fn overhead_manifests_resolve_eight_registered_frames() {
        let mut assets = AssetRuntime::new();
        let accept = OverheadSheet::accept(&mut assets).unwrap();
        let turn = OverheadSheet::turn(&mut assets).unwrap();
        assert_eq!(accept.frames.len(), 8);
        assert_eq!(turn.frames.len(), 8);
        assert_ne!(accept.frames[3].visual_bounds, turn.frames[3].visual_bounds);
        assert_eq!(assets.resource_count(), 2);
    }
}
