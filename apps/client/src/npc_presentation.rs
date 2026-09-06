//! Optional sprite-sheet presentation for NPCs.
//!
//! This module owns only presentation metadata and playback. NPC gameplay
//! state remains in simulation, while the renderer consumes the resolved
//! frame and the existing generic `AssetRuntime` texture identity.

use std::collections::HashMap;

use serde::Deserialize;

use crate::asset_runtime::AssetRuntime;
use crate::character_presentation::PresentationActivity;
use crate::renderer::{DrawQuad, SpriteTextureId};

const RED_SLIME_MANIFEST: &[u8] =
    include_bytes!("../../../Graphic/creature/redslime/manifest.json");
const RED_SLIME_ATLAS: &[u8] = include_bytes!("../../../Graphic/creature/redslime/redslime.png");
const RED_SLIME_KEY: &str = "creature.red_slime.atlas";
const RED_SLIME_FRAME_SECONDS: f32 = 0.10;
// Four attack frames occupy the readable 0.40 s clip while the authoritative
// one-shot remains active for 0.60 s and holds the final frame afterward.
const RED_SLIME_ATTACK_FRAME_SECONDS: f32 = RED_SLIME_FRAME_SECONDS;

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
    frames: Vec<[[f32; 2]; 4]>,
    clips: HashMap<&'static str, SpriteAnimationClip>,
}

impl SpriteSheet {
    pub(crate) fn red_slime(assets: &mut AssetRuntime) -> Result<Self, String> {
        let manifest: RawManifest =
            serde_json::from_slice(RED_SLIME_MANIFEST).map_err(|err| err.to_string())?;
        if manifest.schema_version != 1
            || manifest.kind != "purgatory_sprite_animation"
            || manifest.id != "creature.red_slime"
            || manifest.atlas != "redslime.png"
            || manifest.authored_facing != "right"
        {
            return Err("unsupported red slime sprite manifest".to_owned());
        }
        let texture = assets.register_png(RED_SLIME_KEY, RED_SLIME_ATLAS)?;
        let resource = assets
            .resource(texture)
            .ok_or_else(|| "red slime atlas was not registered".to_owned())?;
        let (width, height) = (resource.image.width(), resource.image.height());
        if (width, height) != (256, 256)
            || manifest.frame_size_px != [64, 64]
            || width % manifest.frame_size_px[0] != 0
            || height % manifest.frame_size_px[1] != 0
        {
            return Err("red slime atlas must be a 256x256 sheet of 64x64 cells".to_owned());
        }
        let columns = width / manifest.frame_size_px[0];
        let rows = height / manifest.frame_size_px[1];
        let frames: Vec<[[f32; 2]; 4]> = (0..columns * rows)
            .map(|index| {
                let x = index % columns * manifest.frame_size_px[0];
                let y = index / columns * manifest.frame_size_px[1];
                [
                    [x as f32 / width as f32, (y + 64) as f32 / height as f32],
                    [
                        (x + 64) as f32 / width as f32,
                        (y + 64) as f32 / height as f32,
                    ],
                    [(x + 64) as f32 / width as f32, y as f32 / height as f32],
                    [x as f32 / width as f32, y as f32 / height as f32],
                ]
            })
            .collect();

        let mut clips = HashMap::new();
        for (name, raw) in manifest.clips {
            let mode = if raw.looped {
                SpritePlaybackMode::Loop
            } else {
                SpritePlaybackMode::Once
            };
            let frame_seconds = if name == "attack" {
                RED_SLIME_ATTACK_FRAME_SECONDS
            } else {
                RED_SLIME_FRAME_SECONDS
            };
            let clip = SpriteAnimationClip::new(raw.frames, frame_seconds, mode);
            if clip
                .frames
                .iter()
                .any(|frame| usize::from(*frame) >= frames.len())
            {
                return Err(format!("red slime clip {name} references an invalid frame"));
            }
            let name = match name.as_str() {
                "idle" => "idle",
                "move" => "move",
                "attack" => "attack",
                _ => return Err(format!("unsupported red slime clip {name}")),
            };
            clips.insert(name, clip);
        }
        for required in ["idle", "move", "attack"] {
            if !clips.contains_key(required) {
                return Err(format!("red slime manifest is missing {required}"));
            }
        }
        Ok(Self {
            texture,
            width,
            height,
            frame_width: 64,
            frame_height: 64,
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
        let half = 0.5;
        let mut quad = DrawQuad::textured_sprite(
            self.texture,
            position,
            [[-half, -half], [half, -half], [half, half], [-half, half]],
            self.frame_uv(frame),
            0.0,
        );
        if flash {
            quad.color = [1.0, 0.72, 0.16, 1.0];
        }
        if facing_left {
            quad = quad.mirror_x_about(position);
        }
        quad
    }
}

#[derive(Deserialize)]
struct RawManifest {
    frame_size_px: [u32; 2],
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
        SpriteSheet::red_slime(&mut assets).unwrap()
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
        player.advance(clip, RED_SLIME_FRAME_SECONDS * 4.0);
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
            player.advance(clip, RED_SLIME_ATTACK_FRAME_SECONDS);
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
        player.advance(clip, RED_SLIME_FRAME_SECONDS);
        let frame = player.frame(clip).unwrap();
        let quad = sheet.quad([0.0, 0.0], frame, true, false);
        assert_eq!(frame, 1);
        assert_eq!(quad.sprite_texture_id(), Some(sheet.texture));
        assert_eq!(quad.color, [1.0, 0.72, 0.16, 1.0]);
    }

    #[test]
    fn manifest_metadata_is_validated_by_the_resolved_sheet() {
        let raw: RawManifest = serde_json::from_slice(RED_SLIME_MANIFEST).unwrap();
        assert_eq!(raw.schema_version, 1);
        assert_eq!(raw.kind, "purgatory_sprite_animation");
        assert_eq!(raw.id, "creature.red_slime");
        assert_eq!(raw.atlas, "redslime.png");
        assert_eq!(raw.authored_facing, "right");
    }
}
