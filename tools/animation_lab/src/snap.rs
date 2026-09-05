//! Editor-only time snapping. Storage still uses the 0.01s document grid.

use crate::document::{AnimDocument, KEY_TIME_GRID, KeyRef, clamp_key_time, times_equal};

pub const GRID_STEPS: [f32; 4] = [0.01, 0.02, 0.05, 0.10];
pub const JOINT_SNAP_STEPS_DEG: [f32; 4] = [5.0, 10.0, 15.0, 30.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapTarget {
    Key,
    Marker,
    Grid,
}

#[derive(Clone, Copy, Debug)]
pub struct SnapSettings {
    pub enabled: bool,
    pub grid: bool,
    pub keys: bool,
    pub markers: bool,
    pub grid_step: f32,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            grid: true,
            keys: true,
            markers: true,
            grid_step: KEY_TIME_GRID,
        }
    }
}

/// Viewport joint-rotation snap. Independent of timeline time snap. Editor state only.
#[derive(Clone, Copy, Debug)]
pub struct JointSnapSettings {
    pub enabled: bool,
    pub step_deg: f32,
}

impl Default for JointSnapSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            step_deg: 15.0,
        }
    }
}

/// Snap a local rotation (radians) to `step_deg`. Off / non-finite → unchanged.
#[must_use]
pub fn snap_joint_rotation(radians: f32, settings: JointSnapSettings) -> f32 {
    if !settings.enabled || !radians.is_finite() {
        return radians;
    }
    let step_deg = if JOINT_SNAP_STEPS_DEG.contains(&settings.step_deg) {
        settings.step_deg
    } else {
        15.0
    };
    let step = step_deg.to_radians();
    if step <= 0.0 {
        return radians;
    }
    (radians / step).round() * step
}

/// Snap `t` using deterministic precedence: existing key, then marker, then grid.
///
/// `ignore` times (the keys being moved) are not snap targets. If the winner
/// would duplicate an ignored-channel destination, the next candidate is tried;
/// if none remain, the 0.01s-quantized unsnapped time is returned.
#[must_use]
pub fn snap_time(
    t: f32,
    duration: f32,
    settings: SnapSettings,
    doc: &AnimDocument,
    ignore: &[KeyRef],
    dest_channel: Option<KeyRef>,
) -> f32 {
    let raw = clamp_key_time(t, duration);
    if !settings.enabled {
        return raw;
    }
    let mut candidates: Vec<(SnapTarget, f32)> = Vec::new();
    if settings.keys {
        for track in &doc.tracks {
            for kind in crate::document::ChannelKind::ALL {
                for key in track.channel(kind) {
                    let skip = ignore.iter().any(|r| {
                        r.bone == track.bone && r.kind == kind && times_equal(r.time(), key.time)
                    });
                    if skip {
                        continue;
                    }
                    candidates.push((SnapTarget::Key, key.time));
                }
            }
        }
    }
    if settings.markers {
        for marker in &doc.markers {
            candidates.push((SnapTarget::Marker, marker.time));
        }
    }
    if settings.grid {
        let step = if GRID_STEPS.contains(&settings.grid_step) {
            settings.grid_step
        } else {
            KEY_TIME_GRID
        };
        let g = ((raw / step).round() * step).clamp(0.0, duration);
        candidates.push((SnapTarget::Grid, clamp_key_time(g, duration)));
    }
    candidates.sort_by(|a, b| {
        let pa = precedence(a.0);
        let pb = precedence(b.0);
        pa.cmp(&pb)
            .then_with(|| {
                let da = (a.1 - raw).abs();
                let db = (b.1 - raw).abs();
                da.total_cmp(&db)
            })
            .then_with(|| a.1.total_cmp(&b.1))
    });
    for (kind, time) in candidates {
        let threshold = match kind {
            SnapTarget::Grid => (settings.grid_step * 0.5).max(KEY_TIME_GRID),
            SnapTarget::Key | SnapTarget::Marker => 0.05,
        };
        if (time - raw).abs() > threshold {
            continue;
        }
        if let Some(dest) = dest_channel
            && would_duplicate(doc, dest, time, ignore)
        {
            continue;
        }
        return clamp_key_time(time, duration);
    }
    raw
}

fn precedence(target: SnapTarget) -> u8 {
    match target {
        SnapTarget::Key => 0,
        SnapTarget::Marker => 1,
        SnapTarget::Grid => 2,
    }
}

fn would_duplicate(doc: &AnimDocument, dest: KeyRef, time: f32, ignore: &[KeyRef]) -> bool {
    let Some(track) = doc.track(dest.bone) else {
        return false;
    };
    track.channel(dest.kind).iter().any(|k| {
        times_equal(k.time, time)
            && !ignore.iter().any(|r| {
                r.bone == dest.bone && r.kind == dest.kind && times_equal(r.time(), k.time)
            })
    })
}
