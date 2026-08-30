//! Client UI-runtime state. Not widgets. Not the authoritative InteractionSession.

use purgatory_protocol::{InteractCloseReason, InteractRejectReason, ServerInteract, WireEntityId};

/// Headline kinds for the current interaction session. Matches this runtime
/// machine; does not invent production UI states.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InteractKind {
    #[default]
    Idle,
    Opening,
    Open,
    Closing,
    Closed,
    Rejected,
}

impl InteractKind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Opening => "OPENING",
            Self::Open => "OPEN",
            Self::Closing => "CLOSING",
            Self::Closed => "CLOSED",
            Self::Rejected => "REJECTED",
        }
    }

    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Idle => "●",
            Self::Opening => "◉",
            Self::Open => "●",
            Self::Closing => "◌",
            Self::Closed => "✓",
            Self::Rejected => "✕",
        }
    }
}

/// Presentation mapping of server interaction control. Idle is the default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UIRuntimeState {
    #[default]
    Idle,
    Opening {
        target: WireEntityId,
    },
    Active {
        session_id: u32,
        target: WireEntityId,
    },
    Closing {
        session_id: u32,
        target: WireEntityId,
    },
    Rejected {
        target: WireEntityId,
        reason: InteractRejectReason,
    },
    Closed {
        session_id: u32,
        reason: InteractCloseReason,
    },
}

impl UIRuntimeState {
    #[must_use]
    pub fn kind(self) -> InteractKind {
        match self {
            Self::Idle => InteractKind::Idle,
            Self::Opening { .. } => InteractKind::Opening,
            Self::Active { .. } => InteractKind::Open,
            Self::Closing { .. } => InteractKind::Closing,
            Self::Rejected { .. } => InteractKind::Rejected,
            Self::Closed { .. } => InteractKind::Closed,
        }
    }

    pub fn apply_server(&mut self, event: ServerInteract) {
        *self = match event {
            ServerInteract::Opened { session_id, target } => Self::Active { session_id, target },
            ServerInteract::Updated { session_id, target } => Self::Active { session_id, target },
            ServerInteract::Rejected { target, reason } => Self::Rejected { target, reason },
            ServerInteract::Closed { session_id, reason } => Self::Closed { session_id, reason },
        };
    }

    pub fn begin_open(&mut self, target: WireEntityId) {
        *self = Self::Opening { target };
    }

    pub fn begin_close(&mut self, session_id: u32, target: WireEntityId) {
        *self = Self::Closing { session_id, target };
    }
}

impl std::fmt::Display for UIRuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("Idle"),
            Self::Opening { target } => write!(f, "Opening {target}"),
            Self::Active { session_id, target } => {
                write!(f, "Active session={session_id} target={target}")
            }
            Self::Closing { session_id, target } => {
                write!(f, "Closing session={session_id} target={target}")
            }
            Self::Rejected { target, reason } => write!(f, "Rejected {target} {reason}"),
            Self::Closed { session_id, reason } => {
                write!(f, "Closed session={session_id} {reason}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_becomes_active() {
        let mut state = UIRuntimeState::Idle;
        state.begin_open(WireEntityId {
            index: 1,
            generation: 1,
        });
        assert_eq!(state.kind(), InteractKind::Opening);
        state.apply_server(ServerInteract::Opened {
            session_id: 3,
            target: WireEntityId {
                index: 1,
                generation: 1,
            },
        });
        assert!(matches!(
            state,
            UIRuntimeState::Active { session_id: 3, .. }
        ));
        assert_eq!(state.kind(), InteractKind::Open);
    }

    #[test]
    fn close_request_is_closing_until_server_closed() {
        let mut state = UIRuntimeState::Active {
            session_id: 3,
            target: WireEntityId {
                index: 27,
                generation: 1,
            },
        };
        state.begin_close(
            3,
            WireEntityId {
                index: 27,
                generation: 1,
            },
        );
        assert_eq!(state.kind(), InteractKind::Closing);
        state.apply_server(ServerInteract::Closed {
            session_id: 3,
            reason: InteractCloseReason::Requested,
        });
        assert_eq!(state.kind(), InteractKind::Closed);
    }

    #[test]
    fn rejected_stays_visible_in_debug_state() {
        let mut state = UIRuntimeState::Idle;
        let target = WireEntityId {
            index: 42,
            generation: 1,
        };
        state.begin_open(target);
        assert_eq!(state.to_string(), format!("Opening {target}"));
        state.apply_server(ServerInteract::Rejected {
            target,
            reason: InteractRejectReason::OutOfRange,
        });
        let shown = state.to_string();
        assert!(shown.contains("Rejected"), "{shown}");
        assert!(shown.contains("OutOfRange"), "{shown}");
        assert_eq!(state.kind(), InteractKind::Rejected);
        assert!(matches!(
            state,
            UIRuntimeState::Rejected {
                reason: InteractRejectReason::OutOfRange,
                ..
            }
        ));
    }

    #[test]
    fn closed_stays_visible_in_debug_state() {
        let mut state = UIRuntimeState::Idle;
        state.apply_server(ServerInteract::Closed {
            session_id: 7,
            reason: InteractCloseReason::Requested,
        });
        let shown = state.to_string();
        assert!(shown.contains("Closed"), "{shown}");
        assert!(shown.contains('7'), "{shown}");
        assert_eq!(state.kind(), InteractKind::Closed);
        assert!(matches!(
            state,
            UIRuntimeState::Closed {
                session_id: 7,
                reason: InteractCloseReason::Requested
            }
        ));
    }

    #[test]
    fn address_changed_closes_world_bound_session() {
        let mut state = UIRuntimeState::Active {
            session_id: 9,
            target: WireEntityId {
                index: 27,
                generation: 1,
            },
        };
        state.apply_server(ServerInteract::Closed {
            session_id: 9,
            reason: InteractCloseReason::AddressChanged,
        });
        assert_eq!(state.kind(), InteractKind::Closed);
        assert!(crate::map_fade::world_interaction_cleared_for_membership(
            state.kind().name()
        ));
        assert!(!crate::map_fade::world_interaction_cleared_for_membership(
            InteractKind::Open.name()
        ));
        assert!(!crate::map_fade::world_interaction_cleared_for_membership(
            InteractKind::Opening.name()
        ));
    }
}
