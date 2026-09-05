//! A7.0 dope-sheet geometry. Time is horizontal; authored channels are rows.
//!
//! Pivot convention: the time strip maps `0` at `x0` to `duration` at `x1`.
//! Key diamonds are centered on authored times. The marker row is reserved even
//! when the clip has no markers.

use purgatory_skeleton::BoneIndex;

use crate::document::{AnimDocument, ChannelKind, KeyRef, clamp_key_time};

pub const LABEL_WIDTH: f32 = 148.0;
pub const RULER_H: f32 = 20.0;
pub const MARKER_ROW_H: f32 = 22.0;
pub const ROW_H: f32 = 22.0;
pub const KEY_HIT_PX: f32 = 8.0;

/// Horizontal time mapping for the sheet (excluding the label column).
#[derive(Clone, Copy, Debug)]
pub struct TimelineStrip {
    pub duration: f32,
    pub x0: f32,
    pub x1: f32,
}

impl TimelineStrip {
    #[must_use]
    pub fn from_sheet(sheet_left: f32, sheet_width: f32, duration: f32) -> Self {
        let x0 = sheet_left + LABEL_WIDTH;
        let x1 = sheet_left + sheet_width.max(LABEL_WIDTH + 8.0);
        Self {
            duration: duration.max(0.01),
            x0,
            x1,
        }
    }

    #[must_use]
    pub fn x_of(self, t: f32) -> f32 {
        let w = (self.x1 - self.x0).max(1.0);
        let u = (t / self.duration).clamp(0.0, 1.0);
        self.x0 + u * w
    }

    #[must_use]
    pub fn t_of(self, x: f32) -> f32 {
        let w = (self.x1 - self.x0).max(1.0);
        let u = ((x - self.x0) / w).clamp(0.0, 1.0);
        clamp_key_time(u * self.duration, self.duration)
    }

    #[must_use]
    pub fn contains_x(self, x: f32) -> bool {
        x >= self.x0 && x <= self.x1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelRow {
    pub bone: BoneIndex,
    pub kind: ChannelKind,
}

/// Authored (non-empty) channels, stable bone-index then rot/tx/ty/depth.
/// Selected empty-but-visible channels are included so Add-key has a row.
#[must_use]
pub fn channel_rows(
    doc: &AnimDocument,
    selected_bone: Option<BoneIndex>,
    selected_channel: ChannelKind,
) -> Vec<ChannelRow> {
    let mut rows = Vec::new();
    for track in &doc.tracks {
        for kind in ChannelKind::ALL {
            if !track.channel(kind).is_empty() {
                push_unique(
                    &mut rows,
                    ChannelRow {
                        bone: track.bone,
                        kind,
                    },
                );
            }
        }
    }
    if let Some(bone) = selected_bone
        && doc.channel_visible(bone, selected_channel)
    {
        push_unique(
            &mut rows,
            ChannelRow {
                bone,
                kind: selected_channel,
            },
        );
    }
    rows.sort_by_key(|row| (row.bone.as_u8(), channel_ord(row.kind)));
    rows
}

fn push_unique(rows: &mut Vec<ChannelRow>, row: ChannelRow) {
    if !rows.contains(&row) {
        rows.push(row);
    }
}

#[must_use]
fn channel_ord(kind: ChannelKind) -> u8 {
    match kind {
        ChannelKind::Rotation => 0,
        ChannelKind::TranslationX => 1,
        ChannelKind::TranslationY => 2,
        ChannelKind::DepthAngle => 3,
    }
}

/// Hit a key in the time strip. `body_top` is the Y of the first channel row.
#[must_use]
pub fn hit_channel_key(
    strip: TimelineStrip,
    rows: &[ChannelRow],
    body_top: f32,
    pointer: [f32; 2],
    doc: &AnimDocument,
) -> Option<(BoneIndex, ChannelKind, usize)> {
    if !strip.contains_x(pointer[0]) {
        return None;
    }
    for (row_i, row) in rows.iter().enumerate() {
        let y = body_top + row_i as f32 * ROW_H + ROW_H * 0.5;
        if (pointer[1] - y).abs() > ROW_H * 0.5 {
            continue;
        }
        let Some(track) = doc.track(row.bone) else {
            continue;
        };
        for (ki, key) in track.channel(row.kind).iter().enumerate() {
            let x = strip.x_of(key.time);
            if (pointer[0] - x).abs() <= KEY_HIT_PX && (pointer[1] - y).abs() <= KEY_HIT_PX {
                return Some((row.bone, row.kind, ki));
            }
        }
    }
    None
}

/// Keys whose diamonds lie inside an axis-aligned box (dope-sheet space).
#[must_use]
pub fn keys_in_rect(
    strip: TimelineStrip,
    rows: &[ChannelRow],
    body_top: f32,
    a: [f32; 2],
    b: [f32; 2],
    doc: &AnimDocument,
) -> Vec<(BoneIndex, ChannelKind, usize)> {
    let min_x = a[0].min(b[0]);
    let max_x = a[0].max(b[0]);
    let min_y = a[1].min(b[1]);
    let max_y = a[1].max(b[1]);
    let mut out = Vec::new();
    for (row_i, row) in rows.iter().enumerate() {
        let y = body_top + row_i as f32 * ROW_H + ROW_H * 0.5;
        if y < min_y || y > max_y {
            continue;
        }
        let Some(track) = doc.track(row.bone) else {
            continue;
        };
        for (ki, key) in track.channel(row.kind).iter().enumerate() {
            let x = strip.x_of(key.time);
            if x >= min_x && x <= max_x {
                out.push((row.bone, row.kind, ki));
            }
        }
    }
    out
}

/// Inclusive row/time range from two keys, for Shift-select.
#[must_use]
pub fn keys_in_range(
    rows: &[ChannelRow],
    doc: &AnimDocument,
    from: KeyRef,
    to: KeyRef,
) -> Vec<KeyRef> {
    let i0 = rows
        .iter()
        .position(|r| r.bone == from.bone && r.kind == from.kind);
    let i1 = rows
        .iter()
        .position(|r| r.bone == to.bone && r.kind == to.kind);
    let (Some(a), Some(b)) = (i0, i1) else {
        return vec![from, to];
    };
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let t0 = from.time().min(to.time());
    let t1 = from.time().max(to.time());
    let mut out = Vec::new();
    for row in &rows[lo..=hi] {
        let Some(track) = doc.track(row.bone) else {
            continue;
        };
        for key in track.channel(row.kind) {
            if key.time + 1e-6 >= t0 && key.time - 1e-6 <= t1 {
                out.push(KeyRef::from_key(row.bone, row.kind, key.time));
            }
        }
    }
    out
}

/// Hit a marker diamond on the reserved marker row.
#[must_use]
pub fn hit_marker(
    strip: TimelineStrip,
    marker_mid_y: f32,
    pointer: [f32; 2],
    doc: &AnimDocument,
) -> Option<usize> {
    if !strip.contains_x(pointer[0]) {
        return None;
    }
    if (pointer[1] - marker_mid_y).abs() > MARKER_ROW_H * 0.5 {
        return None;
    }
    for (i, marker) in doc.markers.iter().enumerate() {
        let x = strip.x_of(marker.time);
        if (pointer[0] - x).abs() <= KEY_HIT_PX {
            return Some(i);
        }
    }
    None
}
