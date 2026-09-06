//! Character/action state gate. Reasons stay typed; there is no `can_act: bool`.
//!
//! [`crate::InputGateReason`] is absorbed as [`ActionDenialReason::TransitionLocked`].
//! This does not rewrite the ADR-0042 transition barrier.
//! Phase 9A: the exclusive slot is any live action (Windup / Active / Recovery).

use crate::entity::EntityId;
use crate::input_gate::InputGateReason;
use crate::world::World;

/// Why an action or gated command is denied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionDenialReason {
    MissingOwner,
    Busy,
    TransitionLocked,
    Disconnected,
}

impl ActionDenialReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingOwner => "MissingOwner",
            Self::Busy => "Busy",
            Self::TransitionLocked => "TransitionLocked",
            Self::Disconnected => "Disconnected",
        }
    }
}

/// Gate inputs that live outside World (session/transition lock).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ActionGateContext {
    pub transition: Option<InputGateReason>,
    pub session_bound: bool,
}

impl ActionGateContext {
    #[must_use]
    pub const fn in_world() -> Self {
        Self {
            transition: None,
            session_bound: true,
        }
    }
}

/// Evaluate whether `actor` may start an exclusive action now.
pub fn evaluate_action_gate(
    world: &World,
    actor: EntityId,
    ctx: ActionGateContext,
) -> Result<(), ActionDenialReason> {
    if !ctx.session_bound {
        return Err(ActionDenialReason::Disconnected);
    }
    if ctx.transition.is_some() {
        return Err(ActionDenialReason::TransitionLocked);
    }
    if !world.contains(actor) {
        return Err(ActionDenialReason::MissingOwner);
    }
    if world.active_action(actor).is_some() {
        return Err(ActionDenialReason::Busy);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionEnd, ActionKind, RuntimeEvent, RuntimeSpawnRequest, SimulationTick, Transform, World,
        WorldAddress,
    };

    fn spawn_generic(world: &mut World, x: f32) -> crate::EntityId {
        world
            .spawn(
                RuntimeSpawnRequest::transient_at(WorldAddress::DEV)
                    .with_transform(Transform::from_position([x, 1.0]))
                    .visible(),
            )
            .expect("spawn")
    }

    #[test]
    fn action_request_start_complete() {
        let mut world = World::new();
        let owner = spawn_generic(&mut world, 0.0);
        world.begin_tick(SimulationTick::from_count(1));
        let action = world
            .try_start_action(
                owner,
                ActionKind::Test { token: 1 },
                ActionGateContext::in_world(),
            )
            .expect("start");
        assert_eq!(action.phase, crate::action::ActionPhase::Active);
        let ended = world.end_action(action.id, ActionEnd::Completed).unwrap();
        assert_eq!(ended.phase, crate::action::ActionPhase::Completed);
        assert!(world.active_action(owner).is_none());
    }

    #[test]
    fn action_gate_transition_locked_does_not_create_slot() {
        let mut world = World::new();
        let owner = spawn_generic(&mut world, 0.0);
        world.begin_tick(SimulationTick::from_count(1));
        let ctx = ActionGateContext {
            transition: Some(crate::InputGateReason::MapTransition),
            session_bound: true,
        };
        let denied = world.try_start_action(owner, ActionKind::Test { token: 8 }, ctx);
        assert_eq!(denied, Err(crate::ActionDenialReason::TransitionLocked));
        assert!(world.active_action(owner).is_none());
    }

    #[test]
    fn action_gate_rejects_without_slot() {
        let mut world = World::new();
        let owner = spawn_generic(&mut world, 0.0);
        world.begin_tick(SimulationTick::from_count(1));
        world
            .try_start_action(
                owner,
                ActionKind::Test { token: 1 },
                ActionGateContext::in_world(),
            )
            .unwrap();
        let denied = world.try_start_action(
            owner,
            ActionKind::Test { token: 2 },
            ActionGateContext::in_world(),
        );
        assert_eq!(denied, Err(crate::ActionDenialReason::Busy));
        assert_eq!(
            world.active_action(owner).unwrap().kind,
            ActionKind::Test { token: 1 }
        );
        let events = world.commit_runtime_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, RuntimeEvent::ActionRejected { .. }))
        );
    }

    #[test]
    fn action_cancel_and_no_double_complete() {
        let mut world = World::new();
        let owner = spawn_generic(&mut world, 0.0);
        world.begin_tick(SimulationTick::from_count(1));
        let action = world
            .try_start_action(
                owner,
                ActionKind::Test { token: 3 },
                ActionGateContext::in_world(),
            )
            .unwrap();
        world.end_action(action.id, ActionEnd::Cancelled).unwrap();
        assert!(world.end_action(action.id, ActionEnd::Completed).is_err());
    }

    #[test]
    fn owner_loss_cancels_action() {
        let mut world = World::new();
        let owner = spawn_generic(&mut world, 0.0);
        world.begin_tick(SimulationTick::from_count(1));
        let action = world
            .try_start_action(
                owner,
                ActionKind::Test { token: 4 },
                ActionGateContext::in_world(),
            )
            .unwrap();
        assert!(world.despawn(owner));
        assert!(world.active_action(owner).is_none());
        let events = world.commit_runtime_events();
        assert!(events.iter().any(|e| matches!(
            e,
            RuntimeEvent::ActionEnded { id, end: ActionEnd::Cancelled, .. } if *id == action.id
        )));
    }
}
