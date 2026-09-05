//! Minimal data-driven animation asset loading for A-track proof clips.
//!
//! Assets are versioned and validated into [`AnimationClip`]. Clip sampling
//! runtime never consults marker data; markers are parsed + validated but
//! intentionally not dispatched.

use crate::clip::{AnimationClip, BoneTrack, Channel, Interpolation, Keyframe, LoopPolicy};

use purgatory_skeleton::{BoneIndex, SkeletonDef, humanoid_v0_bone_by_label};

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationMarker {
    /// Sorted time marker within `[0, duration]`.
    pub time: f32,
    /// Human-readable marker name. No gameplay semantics in A6.
    pub name: String,
    /// Marker type / category (presentation notify reserved).
    pub marker_type: String,
    /// Optional small payload reserved for future use. Kept numeric to avoid
    /// function pointers / callbacks / cross-crate dependencies.
    pub payload: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedAnimationAsset {
    pub clip: AnimationClip,
    /// Reserved, parsed + validated timed markers. Never executed.
    pub markers: Vec<AnimationMarker>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationAssetError {
    pub asset: String,
    pub line: usize,
    pub reason: String,
}

impl AnimationAssetError {
    fn new(asset: impl Into<String>, line: usize, reason: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            line,
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for AnimationAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{} {}", self.asset, self.line, self.reason)
    }
}

const SCHEMA_VERSION_V1: u32 = 1;

/// Parse the minimal whitespace token format used by A6 dev assets.
///
/// Grammar (informal):
/// - `schema_version <u32>`
/// - `duration <f32>`
/// - `loop <Once|Loop>`
/// - `track <bone_label>`
///   - `rot <time> <value> <Linear|Step>`
///   - `tx  <time> <value> <Linear|Step>`
///   - `ty  <time> <value> <Linear|Step>`
///   - `depth <time> <radians> <Linear|Step>` (optional; omitted = 0)
/// - `endtrack`
/// - optional `markers` / `endmarkers` section:
///   - `m <time> <name> <type> [payload_u32]`
pub fn parse_animation_asset_v1(
    asset_name: impl Into<String>,
    text: &str,
    def: &SkeletonDef,
) -> Result<ValidatedAnimationAsset, AnimationAssetError> {
    let asset_name = asset_name.into();
    let mut schema_version: Option<u32> = None;
    let mut duration: Option<f32> = None;
    let mut loop_policy: Option<LoopPolicy> = None;

    let mut tracks: Vec<BoneTrack> = Vec::new();
    let mut current_bone: Option<BoneIndex> = None;
    let mut current_rot: Vec<Keyframe> = Vec::new();
    let mut current_tx: Vec<Keyframe> = Vec::new();
    let mut current_ty: Vec<Keyframe> = Vec::new();
    let mut current_depth: Vec<Keyframe> = Vec::new();

    let mut markers: Vec<AnimationMarker> = Vec::new();
    let mut in_markers = false;

    for (line_idx0, raw_line) in text.lines().enumerate() {
        let line_no = line_idx0 + 1;
        let line = raw_line.split_once('#').map(|(a, _)| a).unwrap_or(raw_line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut it = line.split_whitespace();
        let Some(tok) = it.next() else {
            continue;
        };

        if in_markers {
            match tok {
                "endmarkers" => {
                    in_markers = false;
                }
                "m" => {
                    let time = parse_f32(&asset_name, line_no, it.next(), "marker.time")?;
                    if !time.is_finite() {
                        return Err(AnimationAssetError::new(
                            &asset_name,
                            line_no,
                            "non-finite marker.time",
                        ));
                    }
                    let name = parse_string(&asset_name, line_no, it.next(), "marker.name")?;
                    let marker_type = parse_string(&asset_name, line_no, it.next(), "marker.type")?;
                    let payload =
                        parse_optional_u32(&asset_name, line_no, it.next(), "marker.payload")?;

                    let dur = duration.ok_or_else(|| {
                        AnimationAssetError::new(
                            &asset_name,
                            line_no,
                            "markers require duration before markers",
                        )
                    })?;
                    if time < 0.0 || time > dur {
                        return Err(AnimationAssetError::new(
                            &asset_name,
                            line_no,
                            format!("marker.time out of range [0, {dur}]"),
                        ));
                    }
                    // Sorted uniqueness: enforce on insertion.
                    if let Some(prev) = markers.last() {
                        if time < prev.time {
                            return Err(AnimationAssetError::new(
                                &asset_name,
                                line_no,
                                "marker times must be sorted ascending",
                            ));
                        }
                        if time == prev.time {
                            return Err(AnimationAssetError::new(
                                &asset_name,
                                line_no,
                                "marker times must be unique",
                            ));
                        }
                    }

                    markers.push(AnimationMarker {
                        time,
                        name,
                        marker_type,
                        payload,
                    });
                }
                other => {
                    return Err(AnimationAssetError::new(
                        &asset_name,
                        line_no,
                        format!("unexpected token in markers section: {other}"),
                    ));
                }
            }
            continue;
        }

        match tok {
            "schema_version" => {
                let v = parse_u32(&asset_name, line_no, it.next(), "schema_version")?;
                schema_version = Some(v);
            }
            "duration" => {
                let d = parse_f32(&asset_name, line_no, it.next(), "duration")?;
                duration = Some(d);
            }
            "loop" => {
                let v = it.next().ok_or_else(|| {
                    AnimationAssetError::new(&asset_name, line_no, "missing loop policy")
                })?;
                let lp = match v {
                    "Once" => LoopPolicy::Once,
                    "Loop" => LoopPolicy::Loop,
                    other => {
                        return Err(AnimationAssetError::new(
                            &asset_name,
                            line_no,
                            format!("unknown loop policy {other} (want Once|Loop)"),
                        ));
                    }
                };
                loop_policy = Some(lp);
            }
            "track" => {
                let bone_label = it.next().ok_or_else(|| {
                    AnimationAssetError::new(&asset_name, line_no, "missing track bone label")
                })?;
                let bone = humanoid_v0_bone_by_label(bone_label).ok_or_else(|| {
                    AnimationAssetError::new(
                        &asset_name,
                        line_no,
                        format!("unknown bone label '{bone_label}'"),
                    )
                })?;
                if current_bone.is_some() {
                    return Err(AnimationAssetError::new(
                        &asset_name,
                        line_no,
                        "nested track not allowed",
                    ));
                }
                current_bone = Some(bone);
                current_rot.clear();
                current_tx.clear();
                current_ty.clear();
                current_depth.clear();
            }
            "endtrack" => {
                let bone = current_bone.take().ok_or_else(|| {
                    AnimationAssetError::new(&asset_name, line_no, "endtrack without track")
                })?;
                let mut tracks_bone = BoneTrack {
                    bone,
                    rotation: None,
                    translation_x: None,
                    translation_y: None,
                    depth_angle: None,
                };
                if !current_rot.is_empty() {
                    tracks_bone.rotation =
                        Some(Channel::from_keys(std::mem::take(&mut current_rot)));
                }
                if !current_tx.is_empty() {
                    tracks_bone.translation_x =
                        Some(Channel::from_keys(std::mem::take(&mut current_tx)));
                }
                if !current_ty.is_empty() {
                    tracks_bone.translation_y =
                        Some(Channel::from_keys(std::mem::take(&mut current_ty)));
                }
                if !current_depth.is_empty() {
                    tracks_bone.depth_angle =
                        Some(Channel::from_keys(std::mem::take(&mut current_depth)));
                }
                tracks.push(tracks_bone);
            }
            "markers" => {
                in_markers = true;
            }
            "rot" | "tx" | "ty" | "depth" => {
                let bone = current_bone.ok_or_else(|| {
                    AnimationAssetError::new(&asset_name, line_no, "channel key outside track")
                })?;
                let time = parse_f32(&asset_name, line_no, it.next(), "key.time")?;
                let value = parse_f32(&asset_name, line_no, it.next(), "key.value")?;
                let interp = parse_interp(&asset_name, line_no, it.next())?;
                let _ = bone; // only used for better error messages later
                let key = Keyframe {
                    time,
                    value,
                    interpolation: interp,
                };
                match tok {
                    "rot" => current_rot.push(key),
                    "tx" => current_tx.push(key),
                    "ty" => current_ty.push(key),
                    "depth" => current_depth.push(key),
                    _ => unreachable!(),
                }
            }
            other => {
                return Err(AnimationAssetError::new(
                    &asset_name,
                    line_no,
                    format!("unknown token '{other}'"),
                ));
            }
        }
    }

    if in_markers {
        return Err(AnimationAssetError::new(
            &asset_name,
            text.lines().count(),
            "unterminated markers section",
        ));
    }
    if current_bone.is_some() {
        return Err(AnimationAssetError::new(
            &asset_name,
            text.lines().count(),
            "unterminated track (missing endtrack)",
        ));
    }

    let schema_version = schema_version
        .ok_or_else(|| AnimationAssetError::new(&asset_name, 1, "missing schema_version"))?;
    if schema_version != SCHEMA_VERSION_V1 {
        return Err(AnimationAssetError::new(
            &asset_name,
            1,
            format!("unsupported schema_version {schema_version} (want {SCHEMA_VERSION_V1})"),
        ));
    }
    let duration =
        duration.ok_or_else(|| AnimationAssetError::new(&asset_name, 1, "missing duration"))?;
    let loop_policy =
        loop_policy.ok_or_else(|| AnimationAssetError::new(&asset_name, 1, "missing loop"))?;

    let clip = AnimationClip::try_new(def, duration, loop_policy, tracks).map_err(|e| {
        AnimationAssetError::new(&asset_name, 1, format!("clip validation failed: {e:?}"))
    })?;

    Ok(ValidatedAnimationAsset { clip, markers })
}

/// Serialize a validated A6 v1 asset using the same token grammar as parse.
///
/// Comments and original whitespace are not preserved. Unknown schema cannot
/// appear here because parse already rejected it.
pub fn serialize_animation_asset_v1(asset: &ValidatedAnimationAsset) -> String {
    use purgatory_skeleton::HUMANOID_V0_BONE_LABELS;

    let mut out = String::new();
    out.push_str("schema_version 1\n");
    out.push_str(&format!("duration {}\n", fmt_time(asset.clip.duration())));
    out.push_str("loop ");
    out.push_str(match asset.clip.loop_policy() {
        LoopPolicy::Once => "Once",
        LoopPolicy::Loop => "Loop",
    });
    out.push('\n');

    for track in asset.clip.tracks() {
        let label = HUMANOID_V0_BONE_LABELS
            .get(track.bone.as_usize())
            .copied()
            .unwrap_or("unknown");
        out.push('\n');
        out.push_str("track ");
        out.push_str(label);
        out.push('\n');
        write_channel(&mut out, "rot", track.rotation.as_ref());
        write_channel(&mut out, "tx", track.translation_x.as_ref());
        write_channel(&mut out, "ty", track.translation_y.as_ref());
        write_channel(&mut out, "depth", track.depth_angle.as_ref());
        out.push_str("endtrack\n");
    }

    out.push('\n');
    out.push_str("markers\n");
    for marker in &asset.markers {
        out.push_str("m ");
        out.push_str(&fmt_time(marker.time));
        out.push(' ');
        out.push_str(&marker.name);
        out.push(' ');
        out.push_str(&marker.marker_type);
        if let Some(payload) = marker.payload {
            out.push(' ');
            out.push_str(&payload.to_string());
        }
        out.push('\n');
    }
    out.push_str("endmarkers\n");
    out
}

fn write_channel(out: &mut String, name: &str, channel: Option<&Channel>) {
    let Some(channel) = channel else {
        return;
    };
    for key in channel.keys() {
        out.push_str(name);
        out.push(' ');
        out.push_str(&fmt_time(key.time));
        out.push(' ');
        out.push_str(&fmt_f32(key.value));
        out.push(' ');
        out.push_str(match key.interpolation {
            Interpolation::Linear => "Linear",
            Interpolation::Step => "Step",
        });
        out.push('\n');
    }
}

fn fmt_time(t: f32) -> String {
    if !t.is_finite() {
        return t.to_string();
    }
    format!("{:.2}", (t * 100.0).round() / 100.0)
}

fn fmt_f32(v: f32) -> String {
    if !v.is_finite() {
        return v.to_string();
    }
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_string()
    }
}

/// Fallback clip for malformed dev assets: apply bind rotations over time.
///
/// This is intentionally motionless (no translation channels keyed) while
/// still satisfying `AnimationClip` construction invariants.
pub(crate) fn bind_rotation_noop_clip(
    def: &SkeletonDef,
    duration: f32,
    loop_policy: LoopPolicy,
) -> AnimationClip {
    let interp = Interpolation::Linear;
    let mut tracks = Vec::with_capacity(def.bone_count());
    for bone_i in 0..def.bone_count() {
        let bone = BoneIndex::from_u8(bone_i as u8);
        let bind_rot = def
            .bind_local(bone)
            .expect("Humanoid v0 skeleton must have bind locals for all bones")
            .rotation;
        let keys = vec![
            Keyframe {
                time: 0.0,
                value: bind_rot,
                interpolation: interp,
            },
            Keyframe {
                time: duration,
                value: bind_rot,
                interpolation: interp,
            },
        ];
        tracks.push(BoneTrack::rotation_only(bone, keys));
    }

    AnimationClip::try_new(def, duration, loop_policy, tracks)
        .expect("bind/no-animation fallback clip is valid")
}

fn parse_interp(
    asset_name: &str,
    line_no: usize,
    tok: Option<&str>,
) -> Result<Interpolation, AnimationAssetError> {
    let Some(tok) = tok else {
        return Err(AnimationAssetError::new(
            asset_name,
            line_no,
            "missing interpolation",
        ));
    };
    match tok {
        "Linear" => Ok(Interpolation::Linear),
        "Step" => Ok(Interpolation::Step),
        other => Err(AnimationAssetError::new(
            asset_name,
            line_no,
            format!("unknown interpolation '{other}' (want Linear|Step)"),
        )),
    }
}

fn parse_f32(
    asset_name: &str,
    line_no: usize,
    tok: Option<&str>,
    field: &'static str,
) -> Result<f32, AnimationAssetError> {
    let Some(tok) = tok else {
        return Err(AnimationAssetError::new(
            asset_name,
            line_no,
            format!("missing {field}"),
        ));
    };
    tok.parse::<f32>().map_err(|_| {
        AnimationAssetError::new(
            asset_name,
            line_no,
            format!("invalid float for {field}: '{tok}'"),
        )
    })
}

fn parse_u32(
    asset_name: &str,
    line_no: usize,
    tok: Option<&str>,
    field: &'static str,
) -> Result<u32, AnimationAssetError> {
    let Some(tok) = tok else {
        return Err(AnimationAssetError::new(
            asset_name,
            line_no,
            format!("missing {field}"),
        ));
    };
    tok.parse::<u32>().map_err(|_| {
        AnimationAssetError::new(
            asset_name,
            line_no,
            format!("invalid u32 for {field}: '{tok}'"),
        )
    })
}

fn parse_string(
    asset_name: &str,
    line_no: usize,
    tok: Option<&str>,
    field: &'static str,
) -> Result<String, AnimationAssetError> {
    let Some(tok) = tok else {
        return Err(AnimationAssetError::new(
            asset_name,
            line_no,
            format!("missing {field}"),
        ));
    };
    Ok(tok.to_string())
}

fn parse_optional_u32(
    asset_name: &str,
    line_no: usize,
    tok: Option<&str>,
    field: &'static str,
) -> Result<Option<u32>, AnimationAssetError> {
    let Some(tok) = tok else {
        return Ok(None);
    };
    tok.parse::<u32>().map(Some).map_err(|_| {
        AnimationAssetError::new(asset_name, line_no, format!("invalid {field}: '{tok}'"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_skeleton::humanoid_v0;

    #[test]
    fn marker_validation_rejects_out_of_range_time() {
        let def = humanoid_v0();
        let asset = r#"
schema_version 1
duration 0.6
loop Once
track head
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
markers
m 0.7 boom notify 1
endmarkers
"#;

        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("marker.time out of range"));
    }

    #[test]
    fn marker_validation_rejects_non_finite_time() {
        let def = humanoid_v0();
        let asset = r#"
schema_version 1
duration 0.6
loop Once
track head
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
markers
m NaN boom notify 1
endmarkers
"#;

        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("non-finite marker.time"));
    }

    #[test]
    fn marker_validation_rejects_unsorted_or_duplicate_times() {
        let def = humanoid_v0();

        // Unsorted: 0.2 then 0.1
        let asset = r#"
schema_version 1
duration 0.6
loop Once
track head
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
markers
m 0.2 boom notify 1
m 0.1 boom notify 2
endmarkers
"#;
        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("marker times must be sorted"));

        // Duplicate: 0.2 then 0.2
        let asset = r#"
schema_version 1
duration 0.6
loop Once
track head
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
markers
m 0.2 boom notify 1
m 0.2 boom notify 2
endmarkers
"#;
        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("marker times must be unique"));
    }

    #[test]
    fn unknown_bone_label_is_rejected() {
        let def = humanoid_v0();
        let asset = r#"
schema_version 1
duration 0.6
loop Once
track not_a_bone
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
"#;
        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("unknown bone label"));
    }

    #[test]
    fn duplicate_bone_tracks_are_rejected() {
        let def = humanoid_v0();
        let asset = r#"
schema_version 1
duration 0.6
loop Once
track head
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
track head
rot 0.0 0.0 Linear
rot 0.6 0.0 Linear
endtrack
"#;
        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("clip validation failed"));
    }

    #[test]
    fn serialize_round_trip_a4_fall() {
        let def = humanoid_v0();
        let text = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/shared/animations/dev/a4_fall.anim"
        ));
        let parsed = parse_animation_asset_v1("a4_fall.anim", text, def).unwrap();
        let serialized = serialize_animation_asset_v1(&parsed);
        let reparsed = parse_animation_asset_v1("a4_fall.anim", &serialized, def).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn serialize_round_trip_a4_jump() {
        let def = humanoid_v0();
        let text = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/shared/animations/dev/a4_jump.anim"
        ));
        let parsed = parse_animation_asset_v1("a4_jump.anim", text, def).unwrap();
        let serialized = serialize_animation_asset_v1(&parsed);
        let reparsed = parse_animation_asset_v1("a4_jump.anim", &serialized, def).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn empty_clip_serializes_and_reloads() {
        let def = humanoid_v0();
        let clip = crate::AnimationClip::try_new(def, 1.0, crate::LoopPolicy::Loop, Vec::new())
            .expect("empty track list is a valid clip");
        let asset = ValidatedAnimationAsset {
            clip,
            markers: Vec::new(),
        };
        let text = serialize_animation_asset_v1(&asset);
        let reparsed = parse_animation_asset_v1("empty.anim", &text, def).unwrap();
        assert_eq!(reparsed.clip.duration(), 1.0);
        assert_eq!(reparsed.clip.loop_policy(), crate::LoopPolicy::Loop);
        assert!(reparsed.clip.tracks().is_empty());
        assert!(reparsed.markers.is_empty());
    }

    #[test]
    fn unknown_token_is_rejected() {
        let def = humanoid_v0();
        let asset = r#"
schema_version 1
duration 0.6
loop Once
bezier 1 2 3 4
"#;
        let err = parse_animation_asset_v1("test", asset, def).unwrap_err();
        assert!(err.reason.contains("unknown token"));
    }
}
