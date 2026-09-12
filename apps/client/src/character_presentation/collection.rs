//! Visible-character presentation set. Replica world is the visibility source.

use std::collections::{HashMap, HashSet};

use purgatory_animation::{
    A4_TRANSITION_DURATION, AnimationClip, AnimationPlayer, DepthPose, a1_head_rotation_clip,
    a3_idle_clip, a3_move_clip, a4_fall_clip, a4_jump_clip, a5_attack_clip, a5_hurt_clip,
    apply_depth_projection, blend_depth_poses, blend_local_poses, climb_back_clip, dead_clip,
    sample, sample_depth,
};
use purgatory_content::ContentRegistry;
use purgatory_skeleton::{LocalPose, ROOT, WorldPose, evaluate, humanoid_v0};

use crate::dialogue_animation::DialogueAnimationCatalog;

use super::bone_map::BoneTargetMap;
use super::resolve::{BoundAttachment, MissingPresentation, resolve_equipment};
use super::skeleton_input::{SkeletonInput, skeleton_input_from_state};
use super::state::{CharacterPresentationState, EquipmentView, Facing, PresentationActivity};

/// Generational presentation key. Copied from replica identity at the adapter
/// edge so [`CharacterPresentationState`] does not store protocol types.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresentationEntityKey {
    pub index: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogueAnimationRequest<'a> {
    pub revision: u64,
    pub authored_id: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveDialogueAnimation {
    revision: u64,
    authored_id: String,
}

#[derive(Clone, Copy)]
struct ResolvedDialogueAnimation<'a> {
    request: DialogueAnimationRequest<'a>,
    clip: &'a AnimationClip,
}

impl PresentationEntityKey {
    #[must_use]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

/// Map semantic activity to the authored clip. ClimbBack samples `climb_back.anim`.
#[must_use]
pub fn clip_for_playback_activity(activity: PresentationActivity) -> &'static AnimationClip {
    match activity {
        PresentationActivity::Move => a3_move_clip(),
        PresentationActivity::Jump => a4_jump_clip(),
        PresentationActivity::Fall => a4_fall_clip(),
        PresentationActivity::Attack => a5_attack_clip(),
        PresentationActivity::Hurt => a5_hurt_clip(),
        // Dead is persistent semantically, while its authored one-shot holds
        // the final pose after completion.
        PresentationActivity::Dead => dead_clip(),
        PresentationActivity::ClimbBack => climb_back_clip(),
        PresentationActivity::Idle => a3_idle_clip(),
    }
}

pub struct CharacterPresentationEntry {
    epoch: u64,
    state: CharacterPresentationState,
    input: SkeletonInput,
    local: LocalPose,
    world: WorldPose,
    /// Scratch: sampled target pose for the active clip (before blend / depth).
    clip_local: LocalPose,
    clip_depth: DepthPose,
    /// Captured presented pose at activity change (blend source). Unprojected.
    transition_from: LocalPose,
    transition_from_depth: DepthPose,
    depth: DepthPose,
    eval_local: LocalPose,
    equipment_key: EquipmentView,
    bound: Vec<BoundAttachment>,
    missing: Vec<MissingPresentation>,
    hidden_base: u16,
    playback_activity: PresentationActivity,
    dialogue_animation: Option<ActiveDialogueAnimation>,
    player: AnimationPlayer,
    selected_sample_t: f32,
    transitioning: bool,
    transition_elapsed: f32,
}

impl CharacterPresentationEntry {
    #[must_use]
    pub fn state(&self) -> CharacterPresentationState {
        self.state
    }

    #[must_use]
    pub fn skeleton_input(&self) -> SkeletonInput {
        self.input
    }

    #[must_use]
    pub fn prepared(&self) -> PreparedSkeletonView<'_> {
        PreparedSkeletonView {
            input: self.input,
            local: &self.eval_local,
            world: &self.world,
        }
    }

    #[must_use]
    pub fn bound(&self) -> &[BoundAttachment] {
        &self.bound
    }

    #[must_use]
    pub fn missing(&self) -> &[MissingPresentation] {
        &self.missing
    }

    #[must_use]
    pub fn hidden_base(&self) -> u16 {
        self.hidden_base
    }

    #[must_use]
    pub fn playback_activity(&self) -> PresentationActivity {
        self.playback_activity
    }

    #[must_use]
    pub fn dialogue_animation_id(&self) -> Option<&str> {
        self.dialogue_animation
            .as_ref()
            .map(|animation| animation.authored_id.as_str())
    }

    #[must_use]
    pub fn selected_sample_t(&self) -> f32 {
        self.selected_sample_t
    }

    #[must_use]
    pub fn transitioning(&self) -> bool {
        self.transitioning
    }

    #[must_use]
    pub fn transition_elapsed(&self) -> f32 {
        self.transition_elapsed
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PreparedSkeletonView<'a> {
    pub input: SkeletonInput,
    pub local: &'a LocalPose,
    pub world: &'a WorldPose,
}

/// Per-visible-character presentation. Pose buffers are allocated on enter and
/// reused; leave/despawn drops the entry (no ghost). Equipment resolve runs
/// only when [`EquipmentView`] changes. Each entry owns an independent player
/// and A4 transition state.
pub struct CharacterPresentationSet {
    bone_map: BoneTargetMap,
    epoch: u64,
    resolve_count: u64,
    entries: HashMap<PresentationEntityKey, CharacterPresentationEntry>,
    dialogue_animations: DialogueAnimationCatalog,
    reported_missing_dialogue_animations: HashSet<String>,
}

impl Default for CharacterPresentationSet {
    fn default() -> Self {
        Self::new()
    }
}

impl CharacterPresentationSet {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bone_map: BoneTargetMap::bind_humanoid_v0()
                .expect("Humanoid v0 includes every Phase 8B BoneTarget"),
            epoch: 0,
            resolve_count: 0,
            entries: HashMap::new(),
            dialogue_animations: DialogueAnimationCatalog::default(),
            reported_missing_dialogue_animations: HashSet::new(),
        }
    }

    #[must_use]
    pub(crate) fn with_dialogue_animations(dialogue_animations: DialogueAnimationCatalog) -> Self {
        Self {
            dialogue_animations,
            ..Self::new()
        }
    }

    #[must_use]
    pub fn bone_map(&self) -> BoneTargetMap {
        self.bone_map
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn resolve_count(&self) -> u64 {
        self.resolve_count
    }

    #[must_use]
    pub fn get(&self, key: PresentationEntityKey) -> Option<&CharacterPresentationEntry> {
        self.entries.get(&key)
    }

    #[must_use]
    pub fn facing_of(&self, key: PresentationEntityKey) -> Facing {
        self.entries
            .get(&key)
            .map(|e| e.state.facing)
            .unwrap_or_default()
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (PresentationEntityKey, &CharacterPresentationEntry)> {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// Stable far-to-near independent of HashMap iteration.
    pub fn iter_draw_order(
        &self,
    ) -> impl Iterator<Item = (PresentationEntityKey, &CharacterPresentationEntry)> {
        let mut keys: Vec<_> = self.entries.keys().copied().collect();
        keys.sort_unstable();
        keys.into_iter()
            .filter_map(|k| self.entries.get(&k).map(|v| (k, v)))
    }

    /// A1/A2 debug only: sample the hard-coded head clip into one entry and re-evaluate.
    /// Does not drive normal A3/A4 runtime playback.
    pub fn apply_a1_head_sample(&mut self, key: PresentationEntityKey, t: f32) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        let def = humanoid_v0();
        if sample(a1_head_rotation_clip(), t, &mut entry.local).is_err() {
            return;
        }
        entry.depth.fill_zero();
        finish_evaluate(
            def,
            &entry.local,
            &entry.depth,
            &mut entry.eval_local,
            &mut entry.world,
            entry.input.root_position,
        );
    }

    /// Replace membership with the current visible set. Missing keys are dropped.
    /// Advances each visible character's player at most once with `frame_dt`.
    pub fn sync(
        &mut self,
        items: impl IntoIterator<Item = (PresentationEntityKey, CharacterPresentationState)>,
        registry: &ContentRegistry,
        frame_dt: f32,
    ) {
        self.sync_with_dialogue(
            items.into_iter().map(|(key, state)| (key, state, None)),
            registry,
            frame_dt,
        );
    }

    /// N10f: apply optional client-local animation clips to selected visible
    /// characters while retaining the ordinary activity as fallback.
    pub fn sync_with_dialogue<'a>(
        &mut self,
        items: impl IntoIterator<
            Item = (
                PresentationEntityKey,
                CharacterPresentationState,
                Option<DialogueAnimationRequest<'a>>,
            ),
        >,
        registry: &ContentRegistry,
        frame_dt: f32,
    ) {
        self.epoch = self.epoch.wrapping_add(1);
        let epoch = self.epoch;
        let def = humanoid_v0();
        let bone_map = self.bone_map;
        for (key, state, dialogue_request) in items {
            let dialogue_animation = dialogue_request.and_then(|request| {
                let Some(clip) = self.dialogue_animations.clip(request.authored_id) else {
                    if self
                        .reported_missing_dialogue_animations
                        .insert(request.authored_id.to_owned())
                    {
                        eprintln!(
                            "N10_DIALOGUE animation '{}' unavailable; using ordinary NPC presentation",
                            request.authored_id
                        );
                    }
                    return None;
                };
                Some(ResolvedDialogueAnimation { request, clip })
            });
            let mut did_resolve = false;
            match self.entries.get_mut(&key) {
                Some(entry) => {
                    present_entry(entry, state, dialogue_animation, frame_dt);
                    entry.epoch = epoch;
                    if entry.equipment_key != state.equipment {
                        apply_resolve(entry, bone_map, registry, state.equipment);
                        did_resolve = true;
                    }
                }
                None => {
                    let mut player = AnimationPlayer::new();
                    player.set_playing(true);
                    let playback = state.activity;
                    let clip = dialogue_animation
                        .map(|animation| animation.clip)
                        .unwrap_or_else(|| clip_for_playback_activity(playback));
                    let _ = player.advance(frame_dt, clip);
                    let selected_sample_t = player.sample_time(clip);
                    let mut local = LocalPose::from_bind(def);
                    let mut clip_local = LocalPose::from_bind(def);
                    let transition_from = LocalPose::from_bind(def);
                    let mut clip_depth = DepthPose::zeros(def.bone_count());
                    let transition_from_depth = DepthPose::zeros(def.bone_count());
                    let mut depth = DepthPose::zeros(def.bone_count());
                    let mut eval_local = LocalPose::from_bind(def);
                    let mut world = WorldPose::new(def);
                    let input = skeleton_input_from_state(state);
                    clip_local
                        .copy_bind(def)
                        .expect("Humanoid v0 buffers stay sized to the shared definition");
                    let _ = sample(clip, selected_sample_t, &mut clip_local);
                    clip_depth.fill_zero();
                    let _ = sample_depth(clip, selected_sample_t, &mut clip_depth);
                    local
                        .copy_from(&clip_local)
                        .expect("Humanoid v0 buffers stay sized to the shared definition");
                    let _ = depth.copy_from(&clip_depth);
                    finish_evaluate(
                        def,
                        &local,
                        &depth,
                        &mut eval_local,
                        &mut world,
                        input.root_position,
                    );
                    let resolved = resolve_equipment(state.equipment, registry, bone_map);
                    self.entries.insert(
                        key,
                        CharacterPresentationEntry {
                            epoch,
                            state,
                            input,
                            local,
                            world,
                            clip_local,
                            clip_depth,
                            transition_from,
                            transition_from_depth,
                            depth,
                            eval_local,
                            equipment_key: state.equipment,
                            bound: resolved.bound,
                            missing: resolved.missing,
                            hidden_base: resolved.hidden_base,
                            playback_activity: playback,
                            dialogue_animation: dialogue_animation.map(|animation| {
                                ActiveDialogueAnimation {
                                    revision: animation.request.revision,
                                    authored_id: animation.request.authored_id.to_owned(),
                                }
                            }),
                            player,
                            selected_sample_t,
                            transitioning: false,
                            transition_elapsed: 0.0,
                        },
                    );
                    did_resolve = true;
                }
            }
            if did_resolve {
                self.resolve_count = self.resolve_count.saturating_add(1);
            }
        }
        self.entries.retain(|_, entry| entry.epoch == epoch);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

fn present_entry(
    entry: &mut CharacterPresentationEntry,
    state: CharacterPresentationState,
    dialogue_animation: Option<ResolvedDialogueAnimation<'_>>,
    frame_dt: f32,
) {
    let def = humanoid_v0();
    let next = state.activity;
    let next_dialogue_animation = dialogue_animation.map(|animation| ActiveDialogueAnimation {
        revision: animation.request.revision,
        authored_id: animation.request.authored_id.to_owned(),
    });
    if next != entry.playback_activity || next_dialogue_animation != entry.dialogue_animation {
        // Interruptible: capture currently presented pose (may be mid-blend).
        entry
            .transition_from
            .copy_from(&entry.local)
            .expect("Humanoid v0 buffers stay sized to the shared definition");
        let _ = entry.transition_from_depth.copy_from(&entry.depth);
        entry.transition_elapsed = 0.0;
        entry.transitioning = true;
        entry.playback_activity = next;
        entry.dialogue_animation = next_dialogue_animation;
        entry.player.reset();
    }

    let clip = dialogue_animation
        .map(|animation| animation.clip)
        .unwrap_or_else(|| clip_for_playback_activity(entry.playback_activity));
    let _ = entry.player.advance(frame_dt, clip);
    entry.selected_sample_t = entry.player.sample_time(clip);

    entry
        .clip_local
        .copy_bind(def)
        .expect("Humanoid v0 buffers stay sized to the shared definition");
    let _ = sample(clip, entry.selected_sample_t, &mut entry.clip_local);
    entry.clip_depth.fill_zero();
    let _ = sample_depth(clip, entry.selected_sample_t, &mut entry.clip_depth);

    if entry.transitioning {
        entry.transition_elapsed += frame_dt;
        let alpha = (entry.transition_elapsed / A4_TRANSITION_DURATION).clamp(0.0, 1.0);
        blend_local_poses(
            &entry.transition_from,
            &entry.clip_local,
            alpha,
            &mut entry.local,
        )
        .expect("transition pose buffers match Humanoid v0");
        blend_depth_poses(
            &entry.transition_from_depth,
            &entry.clip_depth,
            alpha,
            &mut entry.depth,
        )
        .expect("transition depth buffers match Humanoid v0");
        if alpha >= 1.0 {
            entry.transitioning = false;
            entry
                .local
                .copy_from(&entry.clip_local)
                .expect("Humanoid v0 buffers stay sized to the shared definition");
            let _ = entry.depth.copy_from(&entry.clip_depth);
        }
    } else {
        entry
            .local
            .copy_from(&entry.clip_local)
            .expect("Humanoid v0 buffers stay sized to the shared definition");
        let _ = entry.depth.copy_from(&entry.clip_depth);
    }

    let input = skeleton_input_from_state(state);
    finish_evaluate(
        def,
        &entry.local,
        &entry.depth,
        &mut entry.eval_local,
        &mut entry.world,
        input.root_position,
    );
    entry.state = state;
    entry.input = input;
}

fn apply_resolve(
    entry: &mut CharacterPresentationEntry,
    bone_map: BoneTargetMap,
    registry: &ContentRegistry,
    equipment: EquipmentView,
) {
    let resolved = resolve_equipment(equipment, registry, bone_map);
    entry.equipment_key = equipment;
    entry.bound.clear();
    entry.bound.extend(resolved.bound);
    entry.missing = resolved.missing;
    entry.hidden_base = resolved.hidden_base;
}

fn finish_evaluate(
    def: &purgatory_skeleton::SkeletonDef,
    unprojected: &LocalPose,
    depth: &DepthPose,
    eval_local: &mut LocalPose,
    world: &mut WorldPose,
    root_position: [f32; 2],
) {
    eval_local
        .copy_from(unprojected)
        .expect("Humanoid v0 buffers stay sized to the shared definition");
    let _ = apply_depth_projection(def, depth, eval_local);
    if let Some(root) = eval_local.get_mut(ROOT) {
        root.translation = root_position;
        root.rotation = 0.0;
    }
    evaluate(def, eval_local, world)
        .expect("Humanoid v0 buffers stay sized to the shared definition");
}
