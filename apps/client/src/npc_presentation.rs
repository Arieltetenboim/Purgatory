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
    steps: Vec<SpriteAnimationStep>,
    mode: SpritePlaybackMode,
    annotations: Vec<SpriteAnnotation>,
    total_ms: u64,
}

impl SpriteAnimationClip {
    fn new(frames: Vec<u16>, steps: Vec<SpriteAnimationStep>, mode: SpritePlaybackMode) -> Self {
        let total_ms = steps.iter().map(|step| u64::from(step.duration_ms)).sum();
        Self {
            frames,
            steps,
            mode,
            annotations: Vec::new(),
            total_ms,
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

    fn with_annotations(mut self, annotations: Vec<SpriteAnnotation>) -> Self {
        self.annotations = annotations;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpriteAnimationStep {
    frame: u16,
    duration_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpriteAnnotation {
    name: String,
    socket: Option<String>,
    step_index: usize,
    offset_ms: u32,
    absolute_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpritePresentationMarker {
    pub(crate) name: String,
    pub(crate) socket: Option<String>,
    pub(crate) step_index: usize,
    pub(crate) offset_ms: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct SpriteAnimationPlayer {
    clip_name: String,
    elapsed_ms: f64,
    finished: bool,
    started: bool,
}

impl SpriteAnimationPlayer {
    pub(crate) fn new() -> Self {
        Self {
            clip_name: "idle".to_owned(),
            elapsed_ms: 0.0,
            finished: false,
            started: false,
        }
    }

    pub(crate) fn set_clip(&mut self, name: &str) {
        if self.clip_name != name {
            self.clip_name = name.to_owned();
            self.elapsed_ms = 0.0;
            self.finished = false;
            self.started = false;
        }
    }

    pub(crate) fn advance(
        &mut self,
        clip: &SpriteAnimationClip,
        dt: f32,
    ) -> Vec<SpritePresentationMarker> {
        if clip.steps.is_empty() || !dt.is_finite() || dt <= 0.0 || self.finished {
            return Vec::new();
        }

        let mut markers = Vec::new();
        if !self.started {
            self.started = true;
            append_markers(&mut markers, clip, 0, 0, true);
        }
        let mut remaining_ms = f64::from(dt) * 1000.0;
        while remaining_ms > 0.000_001 {
            let to_boundary = clip.total_ms as f64 - self.elapsed_ms;
            let segment = remaining_ms.min(to_boundary);
            append_markers(
                &mut markers,
                clip,
                self.elapsed_ms as u64,
                (self.elapsed_ms + segment).round() as u64,
                false,
            );
            self.elapsed_ms = (self.elapsed_ms + segment).round();
            remaining_ms -= segment;
            if self.elapsed_ms >= clip.total_ms as f64 {
                if clip.mode == SpritePlaybackMode::Loop {
                    self.elapsed_ms = 0.0;
                    append_markers(&mut markers, clip, 0, 0, true);
                    if remaining_ms <= 0.000_001 {
                        break;
                    }
                } else {
                    self.elapsed_ms = clip.total_ms as f64;
                    self.finished = true;
                    break;
                }
            }
        }
        markers
    }

    pub(crate) fn frame(&self, clip: &SpriteAnimationClip) -> Option<u16> {
        let mut elapsed = 0_u64;
        for step in &clip.steps {
            let end = elapsed + u64::from(step.duration_ms);
            if self.elapsed_ms < end as f64 || end == clip.total_ms {
                return Some(step.frame);
            }
            elapsed = end;
        }
        None
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn finished(&self) -> bool {
        self.finished
    }
}

fn append_markers(
    output: &mut Vec<SpritePresentationMarker>,
    clip: &SpriteAnimationClip,
    start_ms: u64,
    end_ms: u64,
    include_start: bool,
) {
    for annotation in &clip.annotations {
        let crossed = if include_start {
            annotation.absolute_ms >= start_ms && annotation.absolute_ms <= end_ms
        } else {
            annotation.absolute_ms > start_ms && annotation.absolute_ms <= end_ms
        };
        if crossed {
            output.push(SpritePresentationMarker {
                name: annotation.name.clone(),
                socket: annotation.socket.clone(),
                step_index: annotation.step_index,
                offset_ms: annotation.offset_ms,
            });
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SpriteSheet {
    texture: SpriteTextureId,
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,
    #[allow(dead_code)]
    frame_width: u32,
    #[allow(dead_code)]
    frame_height: u32,
    pixels_per_unit: f32,
    authored_facing_left: bool,
    frames: Vec<SpriteFrame>,
    clips: HashMap<String, SpriteAnimationClip>,
}

#[derive(Clone, Debug, PartialEq)]
struct SpriteFrame {
    rect_px: [u32; 4],
    geometry_size_px: [u32; 2],
    origin_px: [i32; 2],
    sockets: HashMap<String, [i32; 2]>,
    uv: [[f32; 2]; 4],
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
            (0..self.frames.len())
                .map(|index| SpriteAnimationStep {
                    frame: index as u16,
                    duration_ms: (self.frame_seconds * 1000.0).round() as u32,
                })
                .collect(),
            SpritePlaybackMode::Loop,
        );
        let _ = player.advance(&clip, dt);
    }

    pub(crate) fn quad(&self, player: &SpriteAnimationPlayer, position: [f32; 2]) -> DrawQuad {
        let clip = SpriteAnimationClip::new(
            (0..self.frames.len()).map(|index| index as u16).collect(),
            (0..self.frames.len())
                .map(|index| SpriteAnimationStep {
                    frame: index as u16,
                    duration_ms: (self.frame_seconds * 1000.0).round() as u32,
                })
                .collect(),
            SpritePlaybackMode::Loop,
        );
        let frame = self.frames[usize::from(player.frame(&clip).unwrap_or(0)) % self.frames.len()];
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

fn find_sprite_manifest(sprite_id: &str) -> Result<(PathBuf, serde_json::Value), String> {
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
        return Ok((manifest_path, header));
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
        let atlas = manifest
            .get("atlas")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{} has no atlas", manifest_path.display()))?;
        let atlas_name = Path::new(atlas);
        if atlas_name.components().count() != 1 {
            return Err(format!("sprite atlas must be a file name: {}", atlas));
        }
        let atlas_path = manifest_path
            .parent()
            .ok_or_else(|| "sprite manifest has no parent directory".to_owned())?
            .join(atlas_name);
        let atlas_bytes = std::fs::read(&atlas_path)
            .map_err(|error| format!("read {}: {error}", atlas_path.display()))?;
        let id = manifest
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{} has no id", manifest_path.display()))?;
        let texture_key = format!("{id}.atlas");
        let texture = assets.register_png(&texture_key, &atlas_bytes)?;
        let resource = assets
            .resource(texture)
            .ok_or_else(|| format!("sprite texture '{id}' was not registered"))?;
        let (width, height) = (resource.image.width(), resource.image.height());
        let normalized = normalize_manifest(manifest, width, height)?;

        Ok(Self {
            texture,
            width,
            height,
            frame_width: normalized.frame_width,
            frame_height: normalized.frame_height,
            pixels_per_unit: normalized.pixels_per_unit,
            authored_facing_left: normalized.authored_facing == "left",
            frames: normalized.frames,
            clips: normalized.clips,
        })
    }

    pub(crate) fn clip(&self, name: &str) -> &SpriteAnimationClip {
        self.clips
            .get(name)
            .or_else(|| self.clips.get("idle"))
            .expect("SpriteSheet construction guarantees an idle clip")
    }

    #[allow(dead_code)]
    pub(crate) fn frame_uv(&self, frame: u16) -> [[f32; 2]; 4] {
        self.frames[usize::from(frame)].uv
    }

    #[allow(dead_code)]
    pub(crate) fn dimensions_px(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[allow(dead_code)]
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
        let frame_data = &self.frames[usize::from(frame)];
        let [width, height] = frame_data.geometry_size_px;
        let [origin_x, origin_y] = frame_data.origin_px;
        let left = -(origin_x as f32) / self.pixels_per_unit;
        let right = (width as f32 - origin_x as f32) / self.pixels_per_unit;
        let bottom = -(height as f32 - origin_y as f32) / self.pixels_per_unit;
        let top = origin_y as f32 / self.pixels_per_unit;
        let mut quad = DrawQuad::textured_sprite(
            self.texture,
            position,
            [[left, bottom], [right, bottom], [right, top], [left, top]],
            frame_data.uv,
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

    #[allow(dead_code)]
    pub(crate) fn socket(&self, frame: u16, name: &str, facing_left: bool) -> Option<[f32; 2]> {
        let frame_data = &self.frames[usize::from(frame)];
        let [socket_x, socket_y] = *frame_data.sockets.get(name)?;
        let [origin_x, origin_y] = frame_data.origin_px;
        let mut local = [
            (socket_x - origin_x) as f32 / self.pixels_per_unit,
            (origin_y - socket_y) as f32 / self.pixels_per_unit,
        ];
        if facing_left != self.authored_facing_left {
            local[0] = -local[0];
        }
        Some(local)
    }
}

#[derive(Deserialize)]
struct RawManifestHeader {
    schema_version: u32,
    kind: String,
    id: String,
    atlas: String,
}

#[derive(Deserialize)]
struct RawV1Manifest {
    #[serde(flatten)]
    header: RawManifestHeader,
    frame_size_px: [u32; 2],
    #[serde(default)]
    grid_size: Option<[u32; 2]>,
    world_size: [f32; 2],
    frame_seconds: f32,
    authored_facing: String,
    clips: HashMap<String, RawV1Clip>,
}

#[derive(Deserialize)]
struct RawV1Clip {
    frames: Vec<u16>,
    #[serde(rename = "loop")]
    looped: bool,
}

#[derive(Deserialize)]
struct RawV2Manifest {
    #[serde(flatten)]
    header: RawManifestHeader,
    pixels_per_unit: f32,
    authored_facing: String,
    frames: Vec<RawV2Frame>,
    clips: HashMap<String, RawV2Clip>,
}

#[derive(Deserialize)]
struct RawV2Frame {
    rect_px: [u32; 4],
    origin_px: [i32; 2],
    #[serde(default)]
    sockets: HashMap<String, [i32; 2]>,
}

#[derive(Deserialize)]
struct RawV2Clip {
    #[serde(rename = "loop")]
    looped: bool,
    steps: Vec<RawV2Step>,
    #[serde(default)]
    annotations: Vec<RawV2Annotation>,
}

#[derive(Deserialize)]
struct RawV2Step {
    frame: u16,
    duration_ms: u32,
}

#[derive(Deserialize)]
struct RawV2Annotation {
    name: String,
    step: usize,
    offset_ms: u32,
    #[serde(default)]
    socket: Option<String>,
}

#[derive(Clone, Debug)]
struct NormalizedManifest {
    authored_facing: String,
    frame_width: u32,
    frame_height: u32,
    pixels_per_unit: f32,
    frames: Vec<SpriteFrame>,
    clips: HashMap<String, SpriteAnimationClip>,
}

fn normalize_manifest(
    value: serde_json::Value,
    atlas_width: u32,
    atlas_height: u32,
) -> Result<NormalizedManifest, String> {
    let schema_version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "sprite manifest has no schema_version".to_owned())?;
    match schema_version {
        1 => normalize_v1(
            serde_json::from_value(value).map_err(|e| e.to_string())?,
            atlas_width,
            atlas_height,
        ),
        2 => normalize_v2(
            serde_json::from_value(value).map_err(|e| e.to_string())?,
            atlas_width,
            atlas_height,
        ),
        _ => Err(format!(
            "unsupported sprite manifest schema_version {schema_version}"
        )),
    }
}

fn validate_header(header: &RawManifestHeader, version: u32) -> Result<(), String> {
    if header.schema_version != version
        || header.kind != "purgatory_sprite_animation"
        || header.id.is_empty()
        || header.atlas.is_empty()
    {
        return Err(format!("unsupported sprite manifest {}", header.id));
    }
    Ok(())
}

fn make_sprite_frame(
    rect_px: [u32; 4],
    origin_px: [i32; 2],
    sockets: HashMap<String, [i32; 2]>,
    atlas_width: u32,
    atlas_height: u32,
) -> Result<SpriteFrame, String> {
    let [x, y, width, height] = rect_px;
    if width == 0
        || height == 0
        || x.checked_add(width).is_none_or(|right| right > atlas_width)
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > atlas_height)
        || sockets.keys().any(String::is_empty)
    {
        return Err("sprite frame rectangle or socket is invalid".to_owned());
    }
    let u0 = x as f32 / atlas_width as f32;
    let v0 = y as f32 / atlas_height as f32;
    let u1 = (x + width) as f32 / atlas_width as f32;
    let v1 = (y + height) as f32 / atlas_height as f32;
    Ok(SpriteFrame {
        rect_px,
        geometry_size_px: [rect_px[2], rect_px[3]],
        origin_px,
        sockets,
        uv: [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
    })
}

fn normalize_v1(
    raw: RawV1Manifest,
    atlas_width: u32,
    atlas_height: u32,
) -> Result<NormalizedManifest, String> {
    validate_header(&raw.header, 1)?;
    let [frame_width, frame_height] = raw.frame_size_px;
    if frame_width == 0
        || frame_height == 0
        || !matches!(raw.authored_facing.as_str(), "left" | "right")
        || !raw
            .world_size
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
        || !raw.frame_seconds.is_finite()
        || raw.frame_seconds <= 0.0
        || frame_width % 2 != 0
        || frame_height % 2 != 0
    {
        return Err(format!(
            "invalid Version 1 sprite manifest {}",
            raw.header.id
        ));
    }
    let [columns, rows] = raw
        .grid_size
        .unwrap_or_else(|| [atlas_width / frame_width, atlas_height / frame_height]);
    if columns == 0
        || rows == 0
        || columns
            .checked_mul(frame_width)
            .is_none_or(|width| width > atlas_width)
        || rows
            .checked_mul(frame_height)
            .is_none_or(|height| height > atlas_height)
    {
        return Err(format!("{} has an invalid Version 1 grid", raw.header.id));
    }
    let ppu_x = frame_width as f32 / raw.world_size[0];
    let ppu_y = frame_height as f32 / raw.world_size[1];
    if (ppu_x - ppu_y).abs() > 1e-5 {
        return Err(format!(
            "{} has unequal Version 1 pixels-per-unit",
            raw.header.id
        ));
    }
    let duration_ms = (raw.frame_seconds * 1000.0).round();
    if !duration_ms.is_finite()
        || duration_ms < 1.0
        || (raw.frame_seconds * 1000.0 - duration_ms).abs() > 1e-4
        || duration_ms > u32::MAX as f32
    {
        return Err(format!(
            "{} has unrepresentable Version 1 timing",
            raw.header.id
        ));
    }
    let mut frames = Vec::new();
    for index in 0..columns * rows {
        let x = (index % columns) * frame_width;
        let y = (index / columns) * frame_height;
        let mut frame = make_sprite_frame(
            [
                x * atlas_width / (columns * frame_width),
                y * atlas_height / (rows * frame_height),
                atlas_width / columns,
                atlas_height / rows,
            ],
            [(frame_width / 2) as i32, (frame_height / 2) as i32],
            HashMap::new(),
            atlas_width,
            atlas_height,
        )?;
        frame.geometry_size_px = [frame_width, frame_height];
        let column = index % columns;
        let row = index / columns;
        let left = column as f32 / columns as f32;
        let right = (column + 1) as f32 / columns as f32;
        let top = row as f32 / rows as f32;
        let bottom = (row + 1) as f32 / rows as f32;
        frame.uv = [[left, bottom], [right, bottom], [right, top], [left, top]];
        frames.push(frame);
    }
    let mut clips = HashMap::new();
    for (name, clip) in raw.clips {
        if name.is_empty() || clip.frames.is_empty() {
            return Err(format!("{} contains an invalid clip", raw.header.id));
        }
        if clip
            .frames
            .iter()
            .any(|frame| usize::from(*frame) >= frames.len())
        {
            return Err(format!(
                "{} clip {name} references an invalid frame",
                raw.header.id
            ));
        }
        let steps = clip
            .frames
            .iter()
            .map(|frame| SpriteAnimationStep {
                frame: *frame,
                duration_ms: duration_ms as u32,
            })
            .collect();
        clips.insert(
            name,
            SpriteAnimationClip::new(
                clip.frames,
                steps,
                if clip.looped {
                    SpritePlaybackMode::Loop
                } else {
                    SpritePlaybackMode::Once
                },
            ),
        );
    }
    if !clips.contains_key("idle") {
        return Err(format!("{} manifest is missing idle", raw.header.id));
    }
    Ok(NormalizedManifest {
        authored_facing: raw.authored_facing,
        frame_width,
        frame_height,
        pixels_per_unit: ppu_x,
        frames,
        clips,
    })
}

fn normalize_v2(
    raw: RawV2Manifest,
    atlas_width: u32,
    atlas_height: u32,
) -> Result<NormalizedManifest, String> {
    validate_header(&raw.header, 2)?;
    if !matches!(raw.authored_facing.as_str(), "left" | "right")
        || !raw.pixels_per_unit.is_finite()
        || raw.pixels_per_unit <= 0.0
        || raw.frames.is_empty()
    {
        return Err(format!(
            "invalid Version 2 sprite manifest {}",
            raw.header.id
        ));
    }
    let frames = raw
        .frames
        .into_iter()
        .map(|frame| {
            make_sprite_frame(
                frame.rect_px,
                frame.origin_px,
                frame.sockets,
                atlas_width,
                atlas_height,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut clips = HashMap::new();
    for (name, raw_clip) in raw.clips {
        if name.is_empty() || raw_clip.steps.is_empty() {
            return Err(format!("{} contains an invalid clip", raw.header.id));
        }
        let mut steps = Vec::with_capacity(raw_clip.steps.len());
        for step in &raw_clip.steps {
            if step.duration_ms == 0 || usize::from(step.frame) >= frames.len() {
                return Err(format!(
                    "{} clip {name} contains an invalid step",
                    raw.header.id
                ));
            }
            steps.push(SpriteAnimationStep {
                frame: step.frame,
                duration_ms: step.duration_ms,
            });
        }
        let mut annotations = Vec::new();
        for annotation in raw_clip.annotations {
            if annotation.name.is_empty()
                || annotation.step >= steps.len()
                || annotation.offset_ms >= steps[annotation.step].duration_ms
                || annotation.socket.as_ref().is_some_and(|socket| {
                    !frames[usize::from(steps[annotation.step].frame)]
                        .sockets
                        .contains_key(socket)
                })
            {
                return Err(format!(
                    "{} clip {name} contains an invalid annotation",
                    raw.header.id
                ));
            }
            let step_start = steps[..annotation.step]
                .iter()
                .map(|step| u64::from(step.duration_ms))
                .sum::<u64>();
            annotations.push(SpriteAnnotation {
                name: annotation.name,
                socket: annotation.socket,
                step_index: annotation.step,
                offset_ms: annotation.offset_ms,
                absolute_ms: step_start + u64::from(annotation.offset_ms),
            });
        }
        annotations.sort_by_key(|annotation| annotation.absolute_ms);
        let clip = SpriteAnimationClip::new(
            steps.iter().map(|step| step.frame).collect(),
            steps,
            if raw_clip.looped {
                SpritePlaybackMode::Loop
            } else {
                SpritePlaybackMode::Once
            },
        )
        .with_annotations(annotations);
        clips.insert(name, clip);
    }
    if !clips.contains_key("idle") {
        return Err(format!("{} manifest is missing idle", raw.header.id));
    }
    let [frame_width, frame_height] = frames
        .first()
        .map(|frame| [frame.rect_px[2], frame.rect_px[3]])
        .unwrap_or([0, 0]);
    Ok(NormalizedManifest {
        authored_facing: raw.authored_facing,
        frame_width,
        frame_height,
        pixels_per_unit: raw.pixels_per_unit,
        frames,
        clips,
    })
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
        PresentationActivity::Hurt => "hit",
        PresentationActivity::Dead => "death",
        PresentationActivity::Move => "move",
        _ => "idle",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_sheet() -> SpriteSheet {
        let manifest = serde_json::json!({
            "schema_version": 2,
            "kind": "purgatory_sprite_animation",
            "id": "creature.synthetic",
            "atlas": "synthetic.png",
            "pixels_per_unit": 64.0,
            "authored_facing": "right",
            "frames": [
                {"rect_px": [10, 20, 60, 60], "origin_px": [30, 60],
                 "sockets": {"projectile_spawn": [55, 25]}},
                {"rect_px": [100, 4, 165, 80], "origin_px": [20, 40],
                 "sockets": {}}
            ],
            "clips": {
                "idle": {"loop": true, "steps": [
                    {"frame": 0, "duration_ms": 100},
                    {"frame": 1, "duration_ms": 250}
                ], "annotations": [
                    {"name": "first", "step": 0, "offset_ms": 50},
                    {"name": "second", "step": 1, "offset_ms": 100}
                ]}
            }
        });
        let normalized = normalize_manifest(manifest, 512, 128).unwrap();
        SpriteSheet {
            texture: SpriteTextureId::from_raw(1),
            width: 512,
            height: 128,
            frame_width: 60,
            frame_height: 60,
            pixels_per_unit: normalized.pixels_per_unit,
            authored_facing_left: false,
            frames: normalized.frames,
            clips: normalized.clips,
        }
    }

    fn runtime_clip(
        frames: &[u16],
        durations: &[u32],
        mode: SpritePlaybackMode,
    ) -> SpriteAnimationClip {
        SpriteAnimationClip::new(
            frames.to_vec(),
            frames
                .iter()
                .copied()
                .zip(durations.iter().copied())
                .map(|(frame, duration_ms)| SpriteAnimationStep { frame, duration_ms })
                .collect(),
            mode,
        )
    }

    fn sheet() -> SpriteSheet {
        let mut assets = AssetRuntime::new();
        SpriteSheet::from_sprite_id(&mut assets, "creature.red_slime").unwrap()
    }

    #[test]
    fn sheet_resolves_declared_four_by_four_grid() {
        let sheet = sheet();
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
        assert_eq!(clip.frames(), &[0, 1, 2, 3, 6, 7]);
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
    fn semantic_activity_names_optional_clips() {
        assert_eq!(
            clip_name(base_activity(0.0, Some(PresentationActivity::Attack))),
            "attack"
        );
        assert_eq!(
            base_activity(0.0, Some(PresentationActivity::Attack)),
            PresentationActivity::Attack
        );
        assert_eq!(clip_name(PresentationActivity::Hurt), "hit");
        assert_eq!(clip_name(PresentationActivity::Dead), "death");
        assert_eq!(clip_name(base_activity(1.0, None)), "move");
        assert_eq!(clip_name(base_activity(0.0, None)), "idle");
    }

    #[test]
    fn missing_optional_clip_falls_back_to_idle() {
        let mut sheet = sheet();
        sheet.clips.remove("move");
        sheet.clips.remove("attack");
        assert_eq!(sheet.clip("move").frames(), sheet.clip("idle").frames());
        assert_eq!(sheet.clip("attack").frames(), sheet.clip("idle").frames());
        assert_eq!(
            sheet.clip("special_attack_3").frames(),
            sheet.clip("idle").frames()
        );
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
    #[test]
    fn manifest_metadata_is_validated_by_the_resolved_sheet() {
        let dir = workspace_root()
            .join("Graphic")
            .join("creature")
            .join("redslime");
        let bytes = std::fs::read(dir.join("manifest.json")).unwrap();
        let raw: RawManifest = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(raw.schema_version, 1);
        assert_eq!(raw.kind, "purgatory_sprite_animation");
        assert_eq!(raw.id, "creature.red_slime");
        assert_eq!(raw.authored_facing, "right");

        // Atlas filename is authored data, not a frozen contract.
        assert!(!raw.atlas.is_empty());
        assert_eq!(Path::new(&raw.atlas).components().count(), 1);

        let atlas_path = dir.join(&raw.atlas);
        assert!(
            atlas_path.is_file(),
            "atlas file missing: {}",
            atlas_path.display()
        );

        let png = std::fs::read(&atlas_path).unwrap();
        assert!(
            png.len() >= 24 && png.starts_with(b"\x89PNG\r\n\x1a\n"),
            "atlas is not a PNG: {}",
            atlas_path.display()
        );

        let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(png[20..24].try_into().unwrap());

        let [fw, fh] = raw.frame_size_px;
        assert!(fw > 0 && fh > 0);

        if let Some([columns, rows]) = raw.grid_size {
            assert!(columns > 0 && rows > 0);
        } else {
            assert_eq!(
                width % fw,
                0,
                "atlas width {width} not divisible by frame width {fw}"
            );
            assert_eq!(
                height % fh,
                0,
                "atlas height {height} not divisible by frame height {fh}"
            );
        }
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

    #[test]
    fn version_two_uses_explicit_rects_and_uvs() {
        let sheet = v2_sheet();
        assert_eq!(sheet.frames[1].rect_px, [100, 4, 165, 80]);
        assert_eq!(
            sheet.frame_uv(1),
            [
                [100.0 / 512.0, 84.0 / 128.0],
                [265.0 / 512.0, 84.0 / 128.0],
                [265.0 / 512.0, 4.0 / 128.0],
                [100.0 / 512.0, 4.0 / 128.0],
            ]
        );
    }

    #[test]
    fn origin_controls_geometry_and_facing_mirrors_anchor() {
        let sheet = v2_sheet();
        let right = sheet.quad([3.0, 4.0], 1, false, false);
        assert_eq!(
            right.world_corners(),
            [
                [2.6875, 3.375],
                [5.265625, 3.375],
                [5.265625, 4.625],
                [2.6875, 4.625],
            ]
        );
        let left = sheet.quad([3.0, 4.0], 1, false, true);
        assert_eq!(
            left.world_corners(),
            [
                [0.734375, 3.375],
                [3.3125, 3.375],
                [3.3125, 4.625],
                [0.734375, 4.625],
            ]
        );
    }

    #[test]
    fn sockets_resolve_from_origin_and_mirror() {
        let sheet = v2_sheet();
        assert_eq!(
            sheet.socket(0, "projectile_spawn", false),
            Some([25.0 / 64.0, 35.0 / 64.0])
        );
        assert_eq!(
            sheet.socket(0, "projectile_spawn", true),
            Some([-25.0 / 64.0, 35.0 / 64.0])
        );
    }

    #[test]
    fn variable_step_durations_loop_and_stop() {
        let clip = runtime_clip(&[0, 1], &[100, 250], SpritePlaybackMode::Loop);
        let mut player = SpriteAnimationPlayer::new();
        assert_eq!(
            player.advance(&clip, 0.099),
            Vec::<SpritePresentationMarker>::new()
        );
        assert_eq!(player.frame(&clip), Some(0));
        let _ = player.advance(&clip, 0.002);
        assert_eq!(player.frame(&clip), Some(1));
        let _ = player.advance(&clip, 0.249);
        assert_eq!(player.frame(&clip), Some(0));
        let _ = player.advance(&clip, 0.101);
        assert_eq!(player.frame(&clip), Some(1));

        let once = runtime_clip(&[0, 1], &[100, 250], SpritePlaybackMode::Once);
        let mut once_player = SpriteAnimationPlayer::new();
        let _ = once_player.advance(&once, 1.0);
        assert_eq!(once_player.frame(&once), Some(1));
        assert!(once_player.finished());
        assert!(once_player.advance(&once, 1.0).is_empty());
    }

    #[test]
    fn annotations_cross_in_order_across_steps_and_loops() {
        let sheet = v2_sheet();
        let clip = sheet.clip("idle");
        let mut player = SpriteAnimationPlayer::new();
        assert!(player.advance(clip, 0.02).is_empty());
        assert_eq!(player.advance(clip, 0.04)[0].name, "first");
        assert_eq!(
            player
                .advance(clip, 0.28)
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            vec!["second"]
        );
        assert_eq!(
            player
                .advance(clip, 0.06)
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            vec!["first"]
        );
        player.set_clip("other");
        player.set_clip("idle");
        assert_eq!(player.advance(clip, 0.06)[0].name, "first");
    }

    #[test]
    fn invalid_version_two_references_are_rejected() {
        let mut value = serde_json::json!({
            "schema_version": 2, "kind": "purgatory_sprite_animation", "id": "bad",
            "atlas": "bad.png", "pixels_per_unit": 64.0, "authored_facing": "right",
            "frames": [{"rect_px": [0, 0, 10, 10], "origin_px": [5, 5]}],
            "clips": {"idle": {"loop": true, "steps": [{"frame": 0, "duration_ms": 1}],
                "annotations": []}}
        });
        value["clips"]["idle"]["steps"][0]["duration_ms"] = serde_json::json!(0);
        assert!(normalize_manifest(value, 20, 20).is_err());
    }
}
