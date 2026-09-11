//! Client-local dialogue state and authored line resolution.
//!
//! This is not the text renderer and not the authoritative dialogue engine.
//! It validates server line identity against the active interaction, then
//! exposes resolved text to presentation consumers such as SpeechBubble.

use purgatory_content::{ContentRegistry, DialogueBeatIndex};
use purgatory_protocol::{ServerDialogueLine, ServerInteract, WireEntityId};

use crate::ui_runtime::UIRuntimeState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogueProjectionError {
    InteractionMismatch,
    UnknownLine,
}

#[derive(Default)]
pub(crate) struct DialogueRuntime {
    active: Option<ServerDialogueLine>,
}

impl DialogueRuntime {
    #[must_use]
    pub(crate) fn active(&self) -> Option<ServerDialogueLine> {
        self.active
    }

    #[must_use]
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn apply_line(
        &mut self,
        line: ServerDialogueLine,
        interaction: UIRuntimeState,
        registry: &ContentRegistry,
    ) -> Result<(), DialogueProjectionError> {
        if !matches!(
            interaction,
            UIRuntimeState::Active { session_id, target }
                if session_id == line.session_id && target == line.target
        ) {
            return Err(DialogueProjectionError::InteractionMismatch);
        }
        let resolved = registry
            .npc_dialogue_presentation_by_id(line.npc_content_id)
            .and_then(|definition| {
                definition.line(
                    DialogueBeatIndex::from_raw(line.beat_index),
                    line.line_index,
                )
            });
        if resolved.is_none() {
            self.active = None;
            return Err(DialogueProjectionError::UnknownLine);
        }
        self.active = Some(line);
        Ok(())
    }

    pub(crate) fn apply_interact(&mut self, event: ServerInteract) {
        match event {
            ServerInteract::Opened { session_id, target }
            | ServerInteract::Updated { session_id, target } => {
                if self
                    .active
                    .is_some_and(|line| line.session_id != session_id || line.target != target)
                {
                    self.active = None;
                }
            }
            ServerInteract::Rejected { .. } | ServerInteract::Closed { .. } => {
                self.active = None;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.active = None;
    }

    pub(crate) fn clear_if_target_missing(
        &mut self,
        mut target_exists: impl FnMut(WireEntityId) -> bool,
    ) {
        if self
            .active
            .is_some_and(|active| !target_exists(active.target))
        {
            self.active = None;
        }
    }

    #[must_use]
    pub(crate) fn text<'a>(&self, registry: &'a ContentRegistry) -> Option<&'a str> {
        let active = self.active?;
        registry
            .npc_dialogue_presentation_by_id(active.npc_content_id)?
            .line(
                DialogueBeatIndex::from_raw(active.beat_index),
                active.line_index,
            )
            .map(|line| line.text.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_common::ContentId;
    use purgatory_content::{LoadMode, default_content_root, load_registry};

    fn traveler_line() -> ServerDialogueLine {
        ServerDialogueLine {
            session_id: 7,
            target: WireEntityId {
                index: 42,
                generation: 1,
            },
            npc_content_id: ContentId::from_raw(20_001),
            beat_index: 0,
            line_index: 0,
        }
    }

    #[test]
    fn resolves_only_for_matching_active_interaction() {
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("content");
        let line = traveler_line();
        let interaction = UIRuntimeState::Active {
            session_id: line.session_id,
            target: line.target,
        };
        let mut runtime = DialogueRuntime::default();
        runtime
            .apply_line(line, interaction, &registry)
            .expect("matching line");
        assert!(
            runtime
                .text(&registry)
                .is_some_and(|text| text.starts_with("If you're looking for food"))
        );

        runtime.apply_interact(ServerInteract::Closed {
            session_id: 7,
            reason: purgatory_protocol::InteractCloseReason::Requested,
        });
        assert!(!runtime.is_active());
    }

    #[test]
    fn rejects_stale_session_and_unknown_line() {
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("content");
        let mut runtime = DialogueRuntime::default();
        let mut line = traveler_line();
        let interaction = UIRuntimeState::Active {
            session_id: 8,
            target: line.target,
        };
        assert_eq!(
            runtime.apply_line(line, interaction, &registry),
            Err(DialogueProjectionError::InteractionMismatch)
        );

        line.line_index = 99;
        assert_eq!(
            runtime.apply_line(
                line,
                UIRuntimeState::Active {
                    session_id: line.session_id,
                    target: line.target,
                },
                &registry,
            ),
            Err(DialogueProjectionError::UnknownLine)
        );
    }

    #[test]
    fn target_disappearance_clears_only_the_local_projection() {
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("content");
        let line = traveler_line();
        let mut runtime = DialogueRuntime::default();
        runtime
            .apply_line(
                line,
                UIRuntimeState::Active {
                    session_id: line.session_id,
                    target: line.target,
                },
                &registry,
            )
            .expect("line");
        runtime.clear_if_target_missing(|_| false);
        assert!(!runtime.is_active());
    }
}
