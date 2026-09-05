//! Editor clipboard. Never serialized; not history.

use purgatory_animation::{Interpolation, Keyframe};
use purgatory_skeleton::{BONE_COUNT, BoneIndex};

use crate::document::{
    AnimDocument, ChannelKind, KeyRef, channel_authorable, clamp_key_time, sampled_channel_value,
    times_equal,
};

#[derive(Clone, Copy, Debug)]
pub struct ClipboardKey {
    pub bone: BoneIndex,
    pub kind: ChannelKind,
    pub rel_time: f32,
    pub value: f32,
    pub interpolation: Interpolation,
}

#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    pub keys: Vec<ClipboardKey>,
}

impl Clipboard {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[must_use]
pub fn copy_keys(doc: &AnimDocument, refs: &[KeyRef]) -> Clipboard {
    let mut keys = Vec::new();
    for key_ref in refs {
        let Some(key) = doc.track(key_ref.bone).and_then(|tr| {
            tr.channel(key_ref.kind)
                .iter()
                .find(|k| times_equal(k.time, key_ref.time()))
        }) else {
            continue;
        };
        keys.push(ClipboardKey {
            bone: key_ref.bone,
            kind: key_ref.kind,
            rel_time: key.time,
            value: key.value,
            interpolation: key.interpolation,
        });
    }
    if keys.is_empty() {
        return Clipboard::default();
    }
    let origin = keys
        .iter()
        .map(|k| k.rel_time)
        .fold(f32::INFINITY, f32::min);
    for key in &mut keys {
        key.rel_time -= origin;
    }
    keys.sort_by(|a, b| {
        a.bone
            .as_u8()
            .cmp(&b.bone.as_u8())
            .then(a.kind.token().cmp(b.kind.token()))
            .then(a.rel_time.total_cmp(&b.rel_time))
    });
    Clipboard { keys }
}

/// Copy authorable channels at `t` as a pose (all relative times 0).
#[must_use]
pub fn copy_pose(doc: &AnimDocument, t: f32) -> Clipboard {
    let mut keys = Vec::new();
    for i in 0..BONE_COUNT {
        let bone = BoneIndex::from_u8(i);
        for kind in ChannelKind::ALL {
            if !channel_authorable(bone, kind) {
                continue;
            }
            let value = sampled_channel_value(doc, bone, kind, t)
                .unwrap_or_else(|| default_channel_value(bone, kind));
            keys.push(ClipboardKey {
                bone,
                kind,
                rel_time: 0.0,
                value,
                interpolation: Interpolation::Linear,
            });
        }
    }
    Clipboard { keys }
}

#[must_use]
pub fn default_channel_value(bone: BoneIndex, kind: ChannelKind) -> f32 {
    let def = purgatory_skeleton::humanoid_v0();
    let bind = def
        .bind_local(bone)
        .unwrap_or(purgatory_skeleton::BoneTransform::IDENTITY);
    match kind {
        ChannelKind::Rotation => bind.rotation,
        ChannelKind::TranslationX => bind.translation[0],
        ChannelKind::TranslationY => bind.translation[1],
        ChannelKind::DepthAngle => 0.0,
    }
}

/// Build paste keys at `playhead`. Does not mutate `doc`.
pub fn paste_keys(
    doc: &AnimDocument,
    clipboard: &Clipboard,
    playhead: f32,
) -> Result<Vec<(BoneIndex, ChannelKind, Keyframe)>, String> {
    if clipboard.is_empty() {
        return Err("clipboard is empty".to_string());
    }
    let mut out = Vec::new();
    for key in &clipboard.keys {
        if !channel_authorable(key.bone, key.kind)
            && doc
                .track(key.bone)
                .is_none_or(|tr| tr.channel(key.kind).is_empty())
        {
            return Err(format!(
                "cannot paste {}.{}",
                crate::document::bone_label(key.bone),
                key.kind.token()
            ));
        }
        let time = clamp_key_time(playhead + key.rel_time, doc.duration);
        if !key.value.is_finite() {
            return Err("non-finite clipboard value".to_string());
        }
        out.push((
            key.bone,
            key.kind,
            Keyframe {
                time,
                value: key.value,
                interpolation: key.interpolation,
            },
        ));
    }
    // Same-channel collisions after clamp are invalid (would silently merge).
    for i in 0..out.len() {
        for j in (i + 1)..out.len() {
            if out[i].0 == out[j].0
                && out[i].1 == out[j].1
                && times_equal(out[i].2.time, out[j].2.time)
            {
                return Err("paste would collapse two keys onto the same time".to_string());
            }
        }
    }
    Ok(out)
}
