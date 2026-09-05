//! Compact skeleton input and bind / animated evaluate. Clips stay in presentation.

use purgatory_animation::{AnimationClip, DepthPose, sample_and_project};
use purgatory_simulation::PLAYER_HALF_EXTENTS;
use purgatory_skeleton::{
    LocalPose, PoseError, ROOT, SkeletonDef, WorldPose, evaluate, humanoid_v0,
};

use super::state::{CharacterPresentationState, Facing, PresentationActivity, PresentationView};

/// Equipment-independent pose input. Skeleton math still sees only Definition + local Pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkeletonInput {
    pub root_position: [f32; 2],
    pub facing: Facing,
    pub activity: PresentationActivity,
    pub view: PresentationView,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSkeleton {
    pub input: SkeletonInput,
    local: LocalPose,
    world: WorldPose,
}

impl PreparedSkeleton {
    #[must_use]
    pub fn local(&self) -> &LocalPose {
        &self.local
    }

    #[must_use]
    pub fn world(&self) -> &WorldPose {
        &self.world
    }
}

/// Body-center AABB → skeleton root (planted feet). Same mapping as S2 debug draw.
#[must_use]
pub fn skeleton_root(body_center: [f32; 2]) -> [f32; 2] {
    [body_center[0], body_center[1] - PLAYER_HALF_EXTENTS[1]]
}

#[must_use]
pub fn skeleton_input_from_state(state: CharacterPresentationState) -> SkeletonInput {
    SkeletonInput {
        root_position: skeleton_root(state.pose),
        facing: state.facing,
        activity: state.activity,
        view: state.view,
    }
}

/// Evaluate Humanoid v0 bind pose at `input.root_position` (no clip).
pub fn prepare_skeleton(
    state: CharacterPresentationState,
    local: &mut LocalPose,
    world: &mut WorldPose,
) -> Result<SkeletonInput, PoseError> {
    let def = humanoid_v0();
    let input = skeleton_input_from_state(state);
    apply_skeleton_input(def, &input, local, world)?;
    Ok(input)
}

/// `copy_bind` → sample clip → root adapter last → `evaluate`.
pub fn prepare_skeleton_with_clip(
    state: CharacterPresentationState,
    clip: &AnimationClip,
    sample_t: f32,
    local: &mut LocalPose,
    world: &mut WorldPose,
) -> Result<SkeletonInput, PoseError> {
    let def = humanoid_v0();
    let input = skeleton_input_from_state(state);
    local.copy_bind(def)?;
    let mut depth = DepthPose::zeros(def.bone_count());
    let _ = sample_and_project(def, clip, sample_t, local, &mut depth);
    if let Some(root) = local.get_mut(ROOT) {
        root.translation = input.root_position;
        root.rotation = 0.0;
    }
    evaluate(def, local, world)?;
    Ok(input)
}

pub fn apply_skeleton_input(
    def: &SkeletonDef,
    input: &SkeletonInput,
    local: &mut LocalPose,
    world: &mut WorldPose,
) -> Result<(), PoseError> {
    local.copy_bind(def)?;
    if let Some(root) = local.get_mut(ROOT) {
        root.translation = input.root_position;
        root.rotation = 0.0;
    }
    evaluate(def, local, world)?;
    Ok(())
}

pub fn prepared_from_state(
    state: CharacterPresentationState,
) -> Result<PreparedSkeleton, PoseError> {
    let def = humanoid_v0();
    let mut local = LocalPose::from_bind(def);
    let mut world = WorldPose::new(def);
    let input = prepare_skeleton(state, &mut local, &mut world)?;
    Ok(PreparedSkeleton {
        input,
        local,
        world,
    })
}
