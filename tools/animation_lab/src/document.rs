//! Editable A6 animation document. Schema capability and authoring policy stay distinct.

use purgatory_animation::{
    AnimationClip, AnimationMarker, BoneTrack, Channel, ClipError, Interpolation, Keyframe,
    LoopPolicy, ValidatedAnimationAsset,
};
use purgatory_skeleton::{
    BoneIndex, FOOT_BACK, FOOT_FRONT, HAND_BACK, HAND_FRONT, HEAD, LOWER_ARM_BACK, LOWER_ARM_FRONT,
    LOWER_LEG_BACK, LOWER_LEG_FRONT, PELVIS, ROOT, SkeletonDef, TORSO, UPPER_ARM_BACK,
    UPPER_ARM_FRONT, UPPER_LEG_BACK, UPPER_LEG_FRONT, humanoid_v0,
};

/// Fixed 0.01s key-time storage grid. A7.1 editor snap is separate (`snap.rs`).
pub const KEY_TIME_GRID: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Rotation,
    TranslationX,
    TranslationY,
    DepthAngle,
}

impl ChannelKind {
    pub const ALL: [Self; 4] = [
        Self::Rotation,
        Self::TranslationX,
        Self::TranslationY,
        Self::DepthAngle,
    ];

    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Rotation => "rot",
            Self::TranslationX => "tx",
            Self::TranslationY => "ty",
            Self::DepthAngle => "depth",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rotation => "Rotation",
            Self::TranslationX => "TX",
            Self::TranslationY => "TY",
            Self::DepthAngle => "Depth",
        }
    }
}

/// Stable identity for a key while its time is unchanged (0.01s ticks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyRef {
    pub bone: BoneIndex,
    pub kind: ChannelKind,
    pub time_ticks: i32,
}

impl KeyRef {
    #[must_use]
    pub fn from_key(bone: BoneIndex, kind: ChannelKind, time: f32) -> Self {
        Self {
            bone,
            kind,
            time_ticks: time_ticks(time),
        }
    }

    #[must_use]
    pub fn time(self) -> f32 {
        self.time_ticks as f32 / 100.0
    }
}

#[must_use]
pub fn bone_label(bone: BoneIndex) -> &'static str {
    purgatory_skeleton::HUMANOID_V0_BONE_LABELS
        .get(bone.as_usize())
        .copied()
        .unwrap_or("unknown")
}

#[must_use]
pub fn rotation_authorable(bone: BoneIndex) -> bool {
    bone != ROOT
}

#[must_use]
pub fn translation_authorable(bone: BoneIndex) -> bool {
    bone == PELVIS || bone == TORSO || bone == HEAD
}

#[must_use]
pub fn depth_authorable(bone: BoneIndex) -> bool {
    matches!(
        bone,
        UPPER_ARM_FRONT
            | LOWER_ARM_FRONT
            | HAND_FRONT
            | UPPER_ARM_BACK
            | LOWER_ARM_BACK
            | HAND_BACK
            | UPPER_LEG_FRONT
            | LOWER_LEG_FRONT
            | FOOT_FRONT
            | UPPER_LEG_BACK
            | LOWER_LEG_BACK
            | FOOT_BACK
    )
}

#[must_use]
pub fn channel_authorable(bone: BoneIndex, kind: ChannelKind) -> bool {
    match kind {
        ChannelKind::Rotation => rotation_authorable(bone),
        ChannelKind::TranslationX | ChannelKind::TranslationY => translation_authorable(bone),
        ChannelKind::DepthAngle => depth_authorable(bone),
    }
}

#[must_use]
pub fn quantize_key_time(t: f32) -> f32 {
    if !t.is_finite() {
        return 0.0;
    }
    (t * 100.0).round() / 100.0
}

#[must_use]
pub fn time_ticks(t: f32) -> i32 {
    (quantize_key_time(t) * 100.0).round() as i32
}

#[must_use]
pub fn times_equal(a: f32, b: f32) -> bool {
    time_ticks(a) == time_ticks(b)
}

#[must_use]
pub fn clamp_key_time(t: f32, duration: f32) -> f32 {
    let q = quantize_key_time(t);
    if duration <= 0.0 {
        return 0.0;
    }
    q.clamp(0.0, quantize_key_time(duration).max(KEY_TIME_GRID))
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditableTrack {
    pub bone: BoneIndex,
    pub rotation: Vec<Keyframe>,
    pub translation_x: Vec<Keyframe>,
    pub translation_y: Vec<Keyframe>,
    pub depth_angle: Vec<Keyframe>,
}

impl EditableTrack {
    #[must_use]
    pub fn new(bone: BoneIndex) -> Self {
        Self {
            bone,
            rotation: Vec::new(),
            translation_x: Vec::new(),
            translation_y: Vec::new(),
            depth_angle: Vec::new(),
        }
    }

    #[must_use]
    pub fn channel(&self, kind: ChannelKind) -> &[Keyframe] {
        match kind {
            ChannelKind::Rotation => &self.rotation,
            ChannelKind::TranslationX => &self.translation_x,
            ChannelKind::TranslationY => &self.translation_y,
            ChannelKind::DepthAngle => &self.depth_angle,
        }
    }

    pub fn channel_mut(&mut self, kind: ChannelKind) -> &mut Vec<Keyframe> {
        match kind {
            ChannelKind::Rotation => &mut self.rotation,
            ChannelKind::TranslationX => &mut self.translation_x,
            ChannelKind::TranslationY => &mut self.translation_y,
            ChannelKind::DepthAngle => &mut self.depth_angle,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rotation.is_empty()
            && self.translation_x.is_empty()
            && self.translation_y.is_empty()
            && self.depth_angle.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimDocument {
    pub duration: f32,
    pub loop_policy: LoopPolicy,
    pub tracks: Vec<EditableTrack>,
    pub markers: Vec<AnimationMarker>,
}

impl AnimDocument {
    pub fn empty(duration: f32, loop_policy: LoopPolicy) -> Result<Self, String> {
        let doc = Self {
            duration: quantize_key_time(duration).max(KEY_TIME_GRID),
            loop_policy,
            tracks: Vec::new(),
            markers: Vec::new(),
        };
        doc.validate()?;
        Ok(doc)
    }

    #[must_use]
    pub fn from_asset(asset: &ValidatedAnimationAsset) -> Self {
        let mut tracks = Vec::new();
        for track in asset.clip.tracks() {
            let mut editable = EditableTrack::new(track.bone);
            if let Some(ch) = track.rotation.as_ref() {
                editable.rotation = quantize_key_list(ch.keys());
            }
            if let Some(ch) = track.translation_x.as_ref() {
                editable.translation_x = quantize_key_list(ch.keys());
            }
            if let Some(ch) = track.translation_y.as_ref() {
                editable.translation_y = quantize_key_list(ch.keys());
            }
            if let Some(ch) = track.depth_angle.as_ref() {
                editable.depth_angle = quantize_key_list(ch.keys());
            }
            tracks.push(editable);
        }
        Self {
            duration: quantize_key_time(asset.clip.duration()),
            loop_policy: asset.clip.loop_policy(),
            tracks,
            markers: asset
                .markers
                .iter()
                .cloned()
                .map(|mut m| {
                    m.time = quantize_key_time(m.time);
                    m
                })
                .collect(),
        }
    }

    pub fn to_clip(&self, def: &SkeletonDef) -> Result<AnimationClip, ClipError> {
        let mut tracks = Vec::new();
        for track in &self.tracks {
            if track.is_empty() {
                continue;
            }
            tracks.push(BoneTrack {
                bone: track.bone,
                rotation: nonempty_channel(&track.rotation),
                translation_x: nonempty_channel(&track.translation_x),
                translation_y: nonempty_channel(&track.translation_y),
                depth_angle: nonempty_channel(&track.depth_angle),
            });
        }
        AnimationClip::try_new(def, self.duration, self.loop_policy, tracks)
    }

    pub fn to_asset(&self, def: &SkeletonDef) -> Result<ValidatedAnimationAsset, String> {
        self.validate_markers()?;
        let clip = self
            .to_clip(def)
            .map_err(|e| format!("clip validation failed: {e:?}"))?;
        Ok(ValidatedAnimationAsset {
            clip,
            markers: self.markers.clone(),
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        self.to_asset(humanoid_v0()).map(|_| ())
    }

    fn validate_markers(&self) -> Result<(), String> {
        let mut prev: Option<f32> = None;
        for marker in &self.markers {
            if !marker.time.is_finite() {
                return Err("non-finite marker.time".to_string());
            }
            if marker.time < 0.0 || marker.time > self.duration {
                return Err(format!("marker.time out of range [0, {}]", self.duration));
            }
            if let Some(p) = prev {
                if marker.time < p {
                    return Err("marker times must be sorted ascending".to_string());
                }
                if times_equal(marker.time, p) {
                    return Err("marker times must be unique".to_string());
                }
            }
            prev = Some(marker.time);
        }
        Ok(())
    }

    #[must_use]
    pub fn track(&self, bone: BoneIndex) -> Option<&EditableTrack> {
        self.tracks.iter().find(|t| t.bone == bone)
    }

    pub fn track_mut(&mut self, bone: BoneIndex) -> &mut EditableTrack {
        if let Some(i) = self.tracks.iter().position(|t| t.bone == bone) {
            return &mut self.tracks[i];
        }
        self.tracks.push(EditableTrack::new(bone));
        self.tracks.last_mut().expect("just pushed")
    }

    pub fn prune_empty_tracks(&mut self) {
        self.tracks.retain(|t| !t.is_empty());
    }

    /// True when the inspector/timeline may add a new key on this channel.
    #[must_use]
    pub fn can_add_key(&self, bone: BoneIndex, kind: ChannelKind) -> bool {
        channel_authorable(bone, kind)
    }

    /// Existing keys remain visible even when authoring policy would not expose Add.
    #[must_use]
    pub fn channel_visible(&self, bone: BoneIndex, kind: ChannelKind) -> bool {
        if self.can_add_key(bone, kind) {
            return true;
        }
        self.track(bone)
            .is_some_and(|t| !t.channel(kind).is_empty())
    }

    pub fn upsert_key(
        &mut self,
        bone: BoneIndex,
        kind: ChannelKind,
        mut key: Keyframe,
    ) -> Result<(), String> {
        key.time = clamp_key_time(key.time, self.duration);
        if !key.value.is_finite() {
            return Err("non-finite key value".to_string());
        }
        if kind == ChannelKind::DepthAngle
            && key.value.abs() > purgatory_animation::DEPTH_ANGLE_LIMIT + 1e-5
        {
            return Err("depth_angle must be within ±π/2".to_string());
        }
        let existing = self
            .track(bone)
            .map(|t| {
                t.channel(kind)
                    .iter()
                    .any(|k| times_equal(k.time, key.time))
            })
            .unwrap_or(false);
        if !existing && !self.can_add_key(bone, kind) {
            return Err(format!(
                "channel {}.{} is not authorable",
                bone_label(bone),
                kind.token()
            ));
        }
        if !existing && bone == ROOT {
            return Err("root is not keyable".to_string());
        }
        let mut next = self.clone();
        let keys = next.track_mut(bone).channel_mut(kind);
        if let Some(found) = keys.iter_mut().find(|k| times_equal(k.time, key.time)) {
            *found = key;
        } else {
            keys.push(key);
            keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        next.prune_empty_tracks();
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn delete_key(
        &mut self,
        bone: BoneIndex,
        kind: ChannelKind,
        index: usize,
    ) -> Result<Keyframe, String> {
        let mut next = self.clone();
        let keys = next.track_mut(bone).channel_mut(kind);
        if index >= keys.len() {
            return Err("key index out of range".to_string());
        }
        let removed = keys.remove(index);
        next.prune_empty_tracks();
        next.validate()?;
        *self = next;
        Ok(removed)
    }

    pub fn move_key(
        &mut self,
        bone: BoneIndex,
        kind: ChannelKind,
        index: usize,
        new_time: f32,
    ) -> Result<(), String> {
        let new_time = clamp_key_time(new_time, self.duration);
        let mut next = self.clone();
        let keys = next.track_mut(bone).channel_mut(kind);
        if index >= keys.len() {
            return Err("key index out of range".to_string());
        }
        if keys
            .iter()
            .enumerate()
            .any(|(i, k)| i != index && times_equal(k.time, new_time))
        {
            return Err("duplicate key time".to_string());
        }
        keys[index].time = new_time;
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set_duration(&mut self, duration: f32) -> Result<(), String> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err("duration must be finite and positive".to_string());
        }
        let mut next = self.clone();
        next.duration = quantize_key_time(duration).max(KEY_TIME_GRID);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set_loop_policy(&mut self, loop_policy: LoopPolicy) -> Result<(), String> {
        self.loop_policy = loop_policy;
        self.validate()?;
        Ok(())
    }

    pub fn upsert_marker(&mut self, marker: AnimationMarker) -> Result<(), String> {
        let mut marker = marker;
        marker.time = clamp_key_time(marker.time, self.duration);
        if let Some(existing) = self
            .markers
            .iter_mut()
            .find(|m| times_equal(m.time, marker.time))
        {
            *existing = marker;
        } else {
            self.markers.push(marker);
            self.markers.sort_by(|a, b| a.time.total_cmp(&b.time));
        }
        self.validate()?;
        Ok(())
    }

    pub fn delete_marker(&mut self, index: usize) -> Result<AnimationMarker, String> {
        if index >= self.markers.len() {
            return Err("marker index out of range".to_string());
        }
        let removed = self.markers.remove(index);
        self.validate()?;
        Ok(removed)
    }

    pub fn move_marker(&mut self, index: usize, new_time: f32) -> Result<(), String> {
        let new_time = clamp_key_time(new_time, self.duration);
        if index >= self.markers.len() {
            return Err("marker index out of range".to_string());
        }
        if self
            .markers
            .iter()
            .enumerate()
            .any(|(i, m)| i != index && times_equal(m.time, new_time))
        {
            return Err("marker times must be unique".to_string());
        }
        self.markers[index].time = new_time;
        self.markers.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.validate()?;
        Ok(())
    }

    /// Delete many keys. Candidate document is validated once.
    pub fn delete_keys(&mut self, refs: &[KeyRef]) -> Result<(), String> {
        if refs.is_empty() {
            return Ok(());
        }
        let mut next = self.clone();
        for key_ref in refs {
            let keys = next.track_mut(key_ref.bone).channel_mut(key_ref.kind);
            if let Some(i) = keys
                .iter()
                .position(|k| time_ticks(k.time) == key_ref.time_ticks)
            {
                keys.remove(i);
            }
        }
        next.prune_empty_tracks();
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Move many keys. `moves` is `(identity at start, new_time)`. Atomic.
    pub fn move_keys(&mut self, moves: &[(KeyRef, f32)]) -> Result<Vec<KeyRef>, String> {
        if moves.is_empty() {
            return Ok(Vec::new());
        }
        let mut next = self.clone();
        let mut removed: Vec<(BoneIndex, ChannelKind, Keyframe)> = Vec::new();
        for (key_ref, _) in moves {
            let keys = next.track_mut(key_ref.bone).channel_mut(key_ref.kind);
            if let Some(i) = keys
                .iter()
                .position(|k| time_ticks(k.time) == key_ref.time_ticks)
            {
                let key = keys.remove(i);
                removed.push((key_ref.bone, key_ref.kind, key));
            } else {
                return Err("key not found for batch move".to_string());
            }
        }
        let mut inserted = Vec::new();
        for ((_, new_time), (bone, kind, mut key)) in moves.iter().zip(removed) {
            key.time = clamp_key_time(*new_time, next.duration);
            let keys = next.track_mut(bone).channel_mut(kind);
            if keys.iter().any(|k| times_equal(k.time, key.time)) {
                return Err("duplicate key time".to_string());
            }
            keys.push(key);
            keys.sort_by(|a, b| a.time.total_cmp(&b.time));
            inserted.push(KeyRef::from_key(bone, kind, key.time));
        }
        next.prune_empty_tracks();
        next.validate()?;
        *self = next;
        Ok(inserted)
    }

    /// Insert many keys. Atomic: all or nothing.
    pub fn upsert_keys(
        &mut self,
        keys: &[(BoneIndex, ChannelKind, Keyframe)],
    ) -> Result<(), String> {
        let mut next = self.clone();
        for (bone, kind, key) in keys {
            next.upsert_key(*bone, *kind, *key)?;
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Keys whose stored time matches `t` on the 0.01s grid.
    #[must_use]
    pub fn keys_at_time(&self, t: f32) -> Vec<KeyRef> {
        let t = clamp_key_time(t, self.duration);
        let mut refs = Vec::new();
        for track in &self.tracks {
            for kind in ChannelKind::ALL {
                for key in track.channel(kind) {
                    if times_equal(key.time, t) {
                        refs.push(KeyRef::from_key(track.bone, kind, key.time));
                    }
                }
            }
        }
        refs
    }
}

fn nonempty_channel(keys: &[Keyframe]) -> Option<Channel> {
    if keys.is_empty() {
        None
    } else {
        Some(Channel::from_keys(keys.to_vec()))
    }
}

fn quantize_key_list(keys: &[Keyframe]) -> Vec<Keyframe> {
    keys.iter()
        .map(|key| {
            let mut key = *key;
            key.time = quantize_key_time(key.time);
            key
        })
        .collect()
}

#[must_use]
pub fn sampled_channel_value(
    doc: &AnimDocument,
    bone: BoneIndex,
    kind: ChannelKind,
    t: f32,
) -> Option<f32> {
    let keys = doc.track(bone)?.channel(kind);
    if keys.is_empty() {
        return None;
    }
    // Rebuild a tiny clip channel via the runtime sampler by constructing a 1-track clip
    // is heavier than needed; local lookup matches sample_scalar.
    sample_keys(keys, t)
}

fn sample_keys(keys: &[Keyframe], t: f32) -> Option<f32> {
    if keys.is_empty() {
        return None;
    }
    if t <= keys[0].time {
        return Some(keys[0].value);
    }
    let last = keys.len() - 1;
    if t >= keys[last].time {
        return Some(keys[last].value);
    }
    for i in 0..last {
        let a = &keys[i];
        let b = &keys[i + 1];
        if t == a.time {
            return Some(a.value);
        }
        if t < b.time {
            return Some(match a.interpolation {
                Interpolation::Step => a.value,
                Interpolation::Linear => {
                    let span = b.time - a.time;
                    if span <= 0.0 {
                        a.value
                    } else {
                        let u = (t - a.time) / span;
                        a.value + (b.value - a.value) * u
                    }
                }
            });
        }
    }
    Some(keys[last].value)
}
