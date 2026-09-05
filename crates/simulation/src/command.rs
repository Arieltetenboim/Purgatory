//! Authoritative command validation helpers.
//!
//! Command = untrusted request. Event = authoritative occurrence.
//! Invalid commands reject with a typed reason; they do not panic.

use crate::action_gate::ActionDenialReason;
use crate::entity::EntityId;
use crate::input_gate::InputGateReason;

/// Broad command classes that share the session/transition gate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandClass {
    Interact,
    Portal,
    DevChannel,
    Action,
    Ability,
    Equipment,
}

/// Typed command denial. Not a string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandDenial {
    MissingActor,
    TransitionLocked,
    Busy,
    Disconnected,
}

impl CommandDenial {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingActor => "MissingActor",
            Self::TransitionLocked => "TransitionLocked",
            Self::Busy => "Busy",
            Self::Disconnected => "Disconnected",
        }
    }

    #[must_use]
    pub const fn from_action_denial(reason: ActionDenialReason) -> Self {
        match reason {
            ActionDenialReason::MissingOwner => Self::MissingActor,
            ActionDenialReason::Busy => Self::Busy,
            ActionDenialReason::TransitionLocked => Self::TransitionLocked,
            ActionDenialReason::Disconnected => Self::Disconnected,
        }
    }

    #[must_use]
    pub const fn is_gate(self) -> bool {
        matches!(
            self,
            Self::TransitionLocked | Self::Busy | Self::Disconnected
        )
    }
}

/// Session/ownership/transition/busy checks before command-specific validation.
pub fn validate_command_preamble(
    actor: Option<EntityId>,
    transition: Option<InputGateReason>,
    exclusive_busy: bool,
    class: CommandClass,
) -> Result<EntityId, CommandDenial> {
    let Some(actor) = actor else {
        return Err(CommandDenial::Disconnected);
    };
    if transition.is_some() {
        return Err(CommandDenial::TransitionLocked);
    }
    if class == CommandClass::Action && exclusive_busy {
        return Err(CommandDenial::Busy);
    }
    Ok(actor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityId;
    use crate::input_gate::InputGateReason;

    fn actor() -> EntityId {
        EntityId::from_raw(1, 1)
    }

    #[test]
    fn invalid_command_is_typed_reject() {
        assert_eq!(
            validate_command_preamble(None, None, false, CommandClass::Interact),
            Err(CommandDenial::Disconnected)
        );
        assert_eq!(
            validate_command_preamble(
                Some(actor()),
                Some(InputGateReason::MapTransition),
                false,
                CommandClass::Action
            ),
            Err(CommandDenial::TransitionLocked)
        );
        assert_eq!(
            validate_command_preamble(Some(actor()), None, true, CommandClass::Action),
            Err(CommandDenial::Busy)
        );
        assert_eq!(
            validate_command_preamble(Some(actor()), None, true, CommandClass::Interact),
            Ok(actor()),
            "InteractionSession is not the exclusive Action slot"
        );
        assert_eq!(
            validate_command_preamble(Some(actor()), None, true, CommandClass::Equipment),
            Ok(actor()),
            "Equipment is not the exclusive Action slot"
        );
        assert_eq!(
            validate_command_preamble(Some(actor()), None, true, CommandClass::Ability),
            Ok(actor()),
            "Ability busy is owned by request_ability"
        );
    }
}
