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
