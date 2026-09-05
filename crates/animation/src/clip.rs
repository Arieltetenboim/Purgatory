//! Immutable animation clip data and construction-time validation.

use purgatory_skeleton::{BoneIndex, SkeletonDef};

/// Authoring / construction limit for [`BoneTrack::depth_angle`] (`±90°`).
pub const DEPTH_ANGLE_LIMIT: f32 = core::f32::consts::FRAC_PI_2;

/// Floor on projected length factor so parent→child geometry never collapses.
pub const DEPTH_PROJECTION_MIN: f32 = 0.08;

/// Why a clip was rejected. Construction only; `sample` does not allocate this.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipError {
    NonFiniteDuration,
    NonPositiveDuration,
    EmptyChannel { bone: u8 },
    BoneOutOfRange { bone: u8, bone_count: usize },
    DuplicateBoneTrack { bone: u8 },
    NonFiniteKeyTime { bone: u8, key: usize },
    NonFiniteKeyValue { bone: u8, key: usize },
    NegativeKeyTime { bone: u8, key: usize },
    KeyTimeOutOfDuration { bone: u8, key: usize },
    UnsortedKeyTimes { bone: u8, key: usize },
    DuplicateKeyTime { bone: u8, key: usize },
    NoKeyedChannel { bone: u8 },
    DepthAngleOutOfRange { bone: u8, key: usize },
}

/// Playback wrap policy metadata. `sample` ignores wrap; [`crate::AnimationPlayer::sample_time`] applies it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LoopPolicy {
    #[default]
    Once,
    Loop,
}

/// Interpolation from this key toward the next.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Interpolation {
    #[default]
    Linear,
    Step,
}

/// One keyframe on a scalar channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    pub time: f32,
    pub value: f32,
    pub interpolation: Interpolation,
}

/// Contiguous sorted keys for one scalar channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    keys: Box<[Keyframe]>,
}

impl Channel {
    #[must_use]
    pub fn keys(&self) -> &[Keyframe] {
        &self.keys
    }

    /// Construction helper for validated clip authoring.
    ///
    /// Validation of time/value ordering is handled by [`AnimationClip::try_new`],
    /// not here.
    #[must_use]
    pub fn from_keys(keys: Vec<Keyframe>) -> Self {
        Self {
            keys: keys.into_boxed_slice(),
        }
    }
}

/// Sparse per-bone track. Independent optional channels.
#[derive(Clone, Debug, PartialEq)]
pub struct BoneTrack {
    pub bone: BoneIndex,
    pub rotation: Option<Channel>,
    pub translation_x: Option<Channel>,
    pub translation_y: Option<Channel>,
    /// Signed limb depth angle in radians. `0` is full rest projected length.
    pub depth_angle: Option<Channel>,
}

impl BoneTrack {
    /// Rotation-only track helper for A1/dev clips.
    #[must_use]
    pub fn rotation_only(bone: BoneIndex, keys: Vec<Keyframe>) -> Self {
        Self {
            bone,
            rotation: Some(Channel {
                keys: keys.into_boxed_slice(),
            }),
            translation_x: None,
            translation_y: None,
            depth_angle: None,
        }
    }
}

/// Immutable, shareable clip. Contiguous track storage is an implementation detail.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationClip {
    duration: f32,
    loop_policy: LoopPolicy,
    /// v0 compatibility guard: LocalPose length vs the definition used at `try_new`.
    /// Not Skeleton identity, a definition handle, or a rig-version type.
    bone_count: usize,
    tracks: Box<[BoneTrack]>,
}

impl AnimationClip {
    /// Validate and take ownership of track data.
    pub fn try_new(
        def: &SkeletonDef,
        duration: f32,
        loop_policy: LoopPolicy,
        tracks: Vec<BoneTrack>,
    ) -> Result<Self, ClipError> {
        if !duration.is_finite() {
            return Err(ClipError::NonFiniteDuration);
        }
        if duration <= 0.0 {
            return Err(ClipError::NonPositiveDuration);
        }
        let bone_count = def.bone_count();
        let mut seen = vec![false; bone_count];
        for track in &tracks {
            let bi = track.bone.as_usize();
            if bi >= bone_count {
                return Err(ClipError::BoneOutOfRange {
                    bone: track.bone.as_u8(),
                    bone_count,
                });
            }
            if seen[bi] {
                return Err(ClipError::DuplicateBoneTrack {
                    bone: track.bone.as_u8(),
                });
            }
            seen[bi] = true;
            let has_any = track.rotation.is_some()
                || track.translation_x.is_some()
                || track.translation_y.is_some()
                || track.depth_angle.is_some();
            if !has_any {
                return Err(ClipError::NoKeyedChannel {
                    bone: track.bone.as_u8(),
                });
            }
            validate_channel(track.bone, track.rotation.as_ref(), duration)?;
            validate_channel(track.bone, track.translation_x.as_ref(), duration)?;
            validate_channel(track.bone, track.translation_y.as_ref(), duration)?;
            validate_depth_channel(track.bone, track.depth_angle.as_ref(), duration)?;
        }
        Ok(Self {
            duration,
            loop_policy,
            bone_count,
            tracks: tracks.into_boxed_slice(),
        })
    }

    #[must_use]
    pub fn duration(&self) -> f32 {
        self.duration
    }

    #[must_use]
    pub fn loop_policy(&self) -> LoopPolicy {
        self.loop_policy
    }

    /// Length guard from construction. Not a Skeleton identity.
    #[must_use]
    pub fn bone_count(&self) -> usize {
        self.bone_count
    }

    #[must_use]
    pub fn tracks(&self) -> &[BoneTrack] {
        &self.tracks
    }
}

fn validate_channel(
    bone: BoneIndex,
    channel: Option<&Channel>,
    duration: f32,
) -> Result<(), ClipError> {
    let Some(channel) = channel else {
        return Ok(());
    };
    if channel.keys.is_empty() {
        return Err(ClipError::EmptyChannel { bone: bone.as_u8() });
    }
    let mut prev_time: Option<f32> = None;
    for (i, key) in channel.keys.iter().enumerate() {
        if !key.time.is_finite() {
            return Err(ClipError::NonFiniteKeyTime {
                bone: bone.as_u8(),
                key: i,
            });
        }
        if !key.value.is_finite() {
            return Err(ClipError::NonFiniteKeyValue {
                bone: bone.as_u8(),
                key: i,
            });
        }
        if key.time < 0.0 {
            return Err(ClipError::NegativeKeyTime {
                bone: bone.as_u8(),
                key: i,
            });
        }
        if key.time > duration {
            return Err(ClipError::KeyTimeOutOfDuration {
                bone: bone.as_u8(),
                key: i,
            });
        }
        if let Some(prev) = prev_time {
            if key.time < prev {
                return Err(ClipError::UnsortedKeyTimes {
                    bone: bone.as_u8(),
                    key: i,
                });
            }
            if key.time == prev {
                return Err(ClipError::DuplicateKeyTime {
                    bone: bone.as_u8(),
                    key: i,
                });
            }
        }
        prev_time = Some(key.time);
    }
    Ok(())
}

fn validate_depth_channel(
    bone: BoneIndex,
    channel: Option<&Channel>,
    duration: f32,
) -> Result<(), ClipError> {
    validate_channel(bone, channel, duration)?;
    let Some(channel) = channel else {
        return Ok(());
    };
    for (i, key) in channel.keys().iter().enumerate() {
        if key.value.abs() > DEPTH_ANGLE_LIMIT + 1e-5 {
            return Err(ClipError::DepthAngleOutOfRange {
                bone: bone.as_u8(),
                key: i,
            });
        }
    }
    Ok(())
}
