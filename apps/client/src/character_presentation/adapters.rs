//! Thin local/remote source adapters. Difference stops here.

use purgatory_protocol::ReplicatedEquipment;
use purgatory_simulation::EquipmentSlot;

use super::state::{
    CharacterPresentationState, EquipmentView, Facing, PresentationActivity, view_for_activity,
};

/// Horizontal speed below this keeps last facing / idle (wu/s).
pub(crate) const FACE_MOVE_EPS: f32 = 0.5;
/// Vertical speed that distinguishes Jump vs Fall when airborne (wu/s).
/// World +Y is up: rising `vy > AERIAL_EPS`, falling otherwise while airborne.
pub(crate) const AERIAL_EPS: f32 = 0.25;

#[derive(Clone, Copy, Debug)]
pub struct LocalMotion {
    pub pose: [f32; 2],
    pub velocity: [f32; 2],
    pub grounded: bool,
    pub equipment: EquipmentView,
}

#[derive(Clone, Copy, Debug)]
pub struct RemoteMotion {
    pub pose: [f32; 2],
    pub velocity: [f32; 2],
    pub equipment: EquipmentView,
}

#[derive(Clone, Copy, Debug)]
pub struct SocialNpcMotion {
    pub pose: [f32; 2],
    pub equipment: EquipmentView,
}

#[derive(Clone, Copy, Debug)]
struct MotionSample {
    pose: [f32; 2],
    velocity: [f32; 2],
    grounded: Option<bool>,
    equipment: EquipmentView,
}

/// Replica/protocol conversion. Not stored on [`CharacterPresentationState`].
#[must_use]
pub fn equipment_view_from_replica(eq: Option<ReplicatedEquipment>) -> EquipmentView {
    match eq {
        None => EquipmentView::Absent,
        Some(state) => {
            let mut slots = [None; EquipmentSlot::COUNT];
            for slot in EquipmentSlot::ALL {
                slots[slot.index()] = state.get(slot as u8);
            }
            EquipmentView::Present(slots)
        }
    }
}

#[must_use]
pub fn from_local(motion: LocalMotion, held_facing: Facing) -> CharacterPresentationState {
    build_character_presentation(
        MotionSample {
            pose: motion.pose,
            velocity: motion.velocity,
            grounded: Some(motion.grounded),
            equipment: motion.equipment,
        },
        held_facing,
    )
}

#[must_use]
pub fn from_remote(motion: RemoteMotion, held_facing: Facing) -> CharacterPresentationState {
    build_character_presentation(
        MotionSample {
            pose: motion.pose,
            velocity: motion.velocity,
            grounded: None,
            equipment: motion.equipment,
        },
        held_facing,
    )
}

/// Static Social NPC adapter for the shared humanoid presentation path.
///
/// Dialogue-facing and line animation overrides belong to later N10 slices;
/// N10b deliberately starts the ordinary shared presentation in Idle.
#[must_use]
pub fn from_social_npc(motion: SocialNpcMotion, held_facing: Facing) -> CharacterPresentationState {
    let activity = PresentationActivity::Idle;
    CharacterPresentationState {
        pose: motion.pose,
        facing: held_facing,
        activity,
        view: view_for_activity(activity),
        equipment: motion.equipment,
    }
}

fn build_character_presentation(
    motion: MotionSample,
    held_facing: Facing,
) -> CharacterPresentationState {
    let activity = resolve_activity(motion.velocity, motion.grounded);
    CharacterPresentationState {
        pose: motion.pose,
        facing: resolve_facing(motion.velocity[0], held_facing),
        activity,
        view: view_for_activity(activity),
        equipment: motion.equipment,
    }
}

/// DEV / future climb source: replace locomotion with ClimbBack and refresh view.
/// Not a oneshot and not inferred from velocity. Attack/Hurt oneshots should be
/// applied after this if they must win.
#[must_use]
pub fn apply_climb_back_overlay(
    mut state: CharacterPresentationState,
    enable: bool,
) -> CharacterPresentationState {
    if enable {
        state.activity = PresentationActivity::ClimbBack;
        state.view = view_for_activity(state.activity);
    }
    state
}

/// Overlay authoritative Attack/Hurt and persistent Dead on locomotion.
///
/// Precedence (Character Presentation owns this policy):
/// ```text
/// Dead  → overrides Attack, Hurt, and locomotion
/// Hurt / Attack oneshot → replaces Idle/Move/Jump/Fall/ClimbBack
/// locomotion otherwise
/// ```
/// Locomotion never clears an active oneshot (server duration owns that).
/// Presentation does not invent a second gameplay state machine.
#[must_use]
pub fn apply_oneshot_overlay(
    locomotion: PresentationActivity,
    oneshot: Option<PresentationActivity>,
) -> PresentationActivity {
    match oneshot {
        Some(PresentationActivity::Attack) => PresentationActivity::Attack,
        Some(PresentationActivity::Hurt) => PresentationActivity::Hurt,
        Some(_) | None => locomotion,
    }
}

/// Resolve final activity including persistent Dead from Health.
#[must_use]
pub fn resolve_presentation_activity(
    locomotion: PresentationActivity,
    oneshot: Option<PresentationActivity>,
    dead: bool,
) -> PresentationActivity {
    if dead {
        PresentationActivity::Dead
    } else {
        apply_oneshot_overlay(locomotion, oneshot)
    }
}

/// Build presentation state with optional oneshot overlay and Health-derived Dead.
#[must_use]
pub fn from_local_with_oneshot(
    motion: LocalMotion,
    held_facing: Facing,
    oneshot: Option<PresentationActivity>,
) -> CharacterPresentationState {
    from_local_with_presentation(motion, held_facing, oneshot, false)
}

/// Build presentation state with oneshot + optional persistent Dead.
#[must_use]
pub fn from_local_with_presentation(
    motion: LocalMotion,
    held_facing: Facing,
    oneshot: Option<PresentationActivity>,
    dead: bool,
) -> CharacterPresentationState {
    let mut state = from_local(motion, held_facing);
    state.activity = resolve_presentation_activity(state.activity, oneshot, dead);
    state.view = view_for_activity(state.activity);
    state
}

/// Build remote presentation state with an optional authoritative oneshot overlay.
#[must_use]
pub fn from_remote_with_oneshot(
    motion: RemoteMotion,
    held_facing: Facing,
    oneshot: Option<PresentationActivity>,
) -> CharacterPresentationState {
    from_remote_with_presentation(motion, held_facing, oneshot, false)
}

/// Build remote presentation state with oneshot + optional persistent Dead.
#[must_use]
pub fn from_remote_with_presentation(
    motion: RemoteMotion,
    held_facing: Facing,
    oneshot: Option<PresentationActivity>,
    dead: bool,
) -> CharacterPresentationState {
    let mut state = from_remote(motion, held_facing);
    state.activity = resolve_presentation_activity(state.activity, oneshot, dead);
    state.view = view_for_activity(state.activity);
    state
}

#[must_use]
fn resolve_facing(vx: f32, held: Facing) -> Facing {
    if vx > FACE_MOVE_EPS {
        Facing::Right
    } else if vx < -FACE_MOVE_EPS {
        Facing::Left
    } else {
        held
    }
}

/// Infer presentation activity.
///
/// Precedence (airborne wins over Idle/Move):
/// ```text
/// airborne  → rising (vy > AERIAL_EPS) = Jump, else Fall (includes apex)
/// grounded  → |vx| > FACE_MOVE_EPS = Move, else Idle
/// ```
///
/// `grounded = Some(_)` is the local path (must match the velocity source).
/// `grounded = None` is remote: airborne is inferred only when `|vy| > AERIAL_EPS`
/// (no per-remote grounded on the wire).
#[must_use]
pub(crate) fn resolve_activity(velocity: [f32; 2], grounded: Option<bool>) -> PresentationActivity {
    let airborne = match grounded {
        Some(is_grounded) => !is_grounded,
        None => velocity[1].abs() > AERIAL_EPS,
    };
    if airborne {
        if velocity[1] > AERIAL_EPS {
            PresentationActivity::Jump
        } else {
            // Known airborne (local) with small/negative vy, or remote falling.
            PresentationActivity::Fall
        }
    } else if velocity[0].abs() > FACE_MOVE_EPS {
        PresentationActivity::Move
    } else {
        PresentationActivity::Idle
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;

    #[test]
    fn grounded_idle_and_move() {
        assert_eq!(
            resolve_activity([0.0, 0.0], Some(true)),
            PresentationActivity::Idle
        );
        assert_eq!(
            resolve_activity([6.0, 0.0], Some(true)),
            PresentationActivity::Move
        );
        // Grounded must ignore vertical velocity (auth lag / contact noise).
        assert_eq!(
            resolve_activity([0.0, 8.0], Some(true)),
            PresentationActivity::Idle
        );
        assert_eq!(
            resolve_activity([6.0, 8.0], Some(true)),
            PresentationActivity::Move
        );
    }

    #[test]
    fn airborne_jump_and_fall_ignore_horizontal() {
        assert_eq!(
            resolve_activity([0.0, 8.0], Some(false)),
            PresentationActivity::Jump
        );
        assert_eq!(
            resolve_activity([6.0, 8.0], Some(false)),
            PresentationActivity::Jump
        );
        assert_eq!(
            resolve_activity([0.0, -8.0], Some(false)),
            PresentationActivity::Fall
        );
        assert_eq!(
            resolve_activity([6.0, -8.0], Some(false)),
            PresentationActivity::Fall
        );
        // Apex: still airborne, not Idle/Move.
        assert_eq!(
            resolve_activity([6.0, 0.0], Some(false)),
            PresentationActivity::Fall
        );
        assert_eq!(
            resolve_activity([0.0, 0.0], Some(false)),
            PresentationActivity::Fall
        );
    }

    #[test]
    fn landing_returns_to_move_or_idle() {
        assert_eq!(
            resolve_activity([6.0, -1.0], Some(true)),
            PresentationActivity::Move
        );
        assert_eq!(
            resolve_activity([0.0, -1.0], Some(true)),
            PresentationActivity::Idle
        );
    }

    #[test]
    fn remote_infers_airborne_from_vertical_velocity() {
        assert_eq!(
            resolve_activity([6.0, 8.0], None),
            PresentationActivity::Jump
        );
        assert_eq!(
            resolve_activity([6.0, -8.0], None),
            PresentationActivity::Fall
        );
        // Without grounded, small vy cannot prove airborne → Idle/Move.
        assert_eq!(
            resolve_activity([6.0, 0.0], None),
            PresentationActivity::Move
        );
        assert_eq!(
            resolve_activity([0.0, 0.0], None),
            PresentationActivity::Idle
        );
    }

    #[test]
    fn from_local_and_remote_use_same_resolver_rules() {
        let local_jump = from_local(
            LocalMotion {
                pose: [0.0, 1.0],
                velocity: [6.0, 8.0],
                grounded: false,
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
        );
        let remote_jump = from_remote(
            RemoteMotion {
                pose: [0.0, 1.0],
                velocity: [6.0, 8.0],
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
        );
        assert_eq!(local_jump.activity, PresentationActivity::Jump);
        assert_eq!(remote_jump.activity, PresentationActivity::Jump);

        let local_apex = from_local(
            LocalMotion {
                pose: [0.0, 2.0],
                velocity: [6.0, 0.0],
                grounded: false,
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
        );
        assert_eq!(local_apex.activity, PresentationActivity::Fall);
    }

    #[test]
    fn oneshot_overlay_replaces_locomotion() {
        assert_eq!(
            apply_oneshot_overlay(
                PresentationActivity::Move,
                Some(PresentationActivity::Attack)
            ),
            PresentationActivity::Attack
        );
        assert_eq!(
            apply_oneshot_overlay(PresentationActivity::Jump, Some(PresentationActivity::Hurt)),
            PresentationActivity::Hurt
        );
        assert_eq!(
            apply_oneshot_overlay(PresentationActivity::Fall, None),
            PresentationActivity::Fall
        );
        // Non-oneshot Some values are ignored (defensive).
        assert_eq!(
            apply_oneshot_overlay(PresentationActivity::Idle, Some(PresentationActivity::Move)),
            PresentationActivity::Idle
        );
    }

    #[test]
    fn dead_overrides_oneshot_and_locomotion() {
        assert_eq!(
            resolve_presentation_activity(
                PresentationActivity::Move,
                Some(PresentationActivity::Attack),
                true
            ),
            PresentationActivity::Dead
        );
        assert_eq!(
            resolve_presentation_activity(
                PresentationActivity::Idle,
                Some(PresentationActivity::Hurt),
                true
            ),
            PresentationActivity::Dead
        );
        assert_eq!(
            resolve_presentation_activity(PresentationActivity::Jump, None, true),
            PresentationActivity::Dead
        );
        assert_eq!(
            resolve_presentation_activity(
                PresentationActivity::Move,
                Some(PresentationActivity::Attack),
                false
            ),
            PresentationActivity::Attack
        );
    }

    #[test]
    fn from_local_with_oneshot_keeps_pose_and_overlays() {
        let state = from_local_with_oneshot(
            LocalMotion {
                pose: [1.0, 2.0],
                velocity: [6.0, 0.0],
                grounded: true,
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
            Some(PresentationActivity::Attack),
        );
        assert_eq!(state.pose, [1.0, 2.0]);
        assert_eq!(state.activity, PresentationActivity::Attack);
        assert_eq!(state.facing, Facing::Right);
    }

    #[test]
    fn local_and_remote_dead_use_same_mapping() {
        let local = from_local_with_presentation(
            LocalMotion {
                pose: [0.0, 1.0],
                velocity: [6.0, 0.0],
                grounded: true,
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
            Some(PresentationActivity::Attack),
            true,
        );
        let remote = from_remote_with_presentation(
            RemoteMotion {
                pose: [0.0, 1.0],
                velocity: [6.0, 0.0],
                equipment: EquipmentView::Absent,
            },
            Facing::Right,
            Some(PresentationActivity::Hurt),
            true,
        );
        assert_eq!(local.activity, PresentationActivity::Dead);
        assert_eq!(remote.activity, PresentationActivity::Dead);
    }
}
