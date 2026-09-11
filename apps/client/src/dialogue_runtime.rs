//! Client-local dialogue state and authored Beat resolution.
//!
//! This is not the text renderer and not the authoritative dialogue engine.
//! It validates server line identity against the active interaction, then
//! exposes resolved text to presentation consumers such as SpeechBubble.

use purgatory_content::{ContentRegistry, DialogueBeatIndex};
use purgatory_protocol::{
    DialogueChoose, ServerDialogueChoiceAccepted, ServerDialogueLine, ServerInteract, WireEntityId,
};
use std::time::Duration;

use crate::ui_runtime::UIRuntimeState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogueProjectionError {
    InteractionMismatch,
    UnknownBeat,
    UnknownLine,
}

#[derive(Default)]
pub(crate) struct DialogueRuntime {
    active: Option<ServerDialogueLine>,
    pending: Option<ServerDialogueLine>,
    selected_choice: usize,
    accepted: Option<AcceptedChoice>,
    presentation_revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DialoguePresentationCue<'a> {
    pub target: WireEntityId,
    /// Changes whenever a new semantic line becomes visible, including when
    /// the authored animation id is reused by consecutive Beats.
    pub revision: u64,
    pub animation: Option<&'a str>,
}

const ACCEPTED_CHOICE_VISIBLE: Duration = Duration::from_secs(1);

struct AcceptedChoice {
    session_id: u32,
    text: String,
    elapsed: Duration,
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

    #[must_use]
    pub(crate) fn acceptance_pending(&self) -> bool {
        self.accepted.is_some()
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
        let resolved_beat = registry
            .npc_dialogue_presentation_by_id(line.npc_content_id)
            .and_then(|definition| definition.beat(DialogueBeatIndex::from_raw(line.beat_index)));
        let Some(resolved_beat) = resolved_beat else {
            self.active = None;
            return Err(DialogueProjectionError::UnknownBeat);
        };
        if resolved_beat.lines.get(line.line_index as usize).is_none() {
            self.active = None;
            return Err(DialogueProjectionError::UnknownLine);
        }
        if self
            .accepted
            .as_ref()
            .is_some_and(|accepted| accepted.session_id == line.session_id)
        {
            self.pending = Some(line);
        } else {
            self.activate_line(line);
            self.pending = None;
            self.selected_choice = 0;
        }
        Ok(())
    }

    pub(crate) fn apply_choice_accepted(
        &mut self,
        event: ServerDialogueChoiceAccepted,
        registry: &ContentRegistry,
    ) -> Result<(), DialogueProjectionError> {
        let Some(active) = self.active else {
            return Err(DialogueProjectionError::InteractionMismatch);
        };
        if active.session_id != event.session_id || active.beat_index != event.beat_index {
            return Err(DialogueProjectionError::InteractionMismatch);
        }
        let Some(choice) = registry
            .npc_dialogue_presentation_by_id(active.npc_content_id)
            .and_then(|definition| definition.beat(DialogueBeatIndex::from_raw(active.beat_index)))
            .and_then(|beat| beat.choices.get(event.choice_index as usize))
        else {
            return Err(DialogueProjectionError::UnknownBeat);
        };
        self.selected_choice = event.choice_index as usize;
        self.accepted = Some(AcceptedChoice {
            session_id: event.session_id,
            text: choice.text.clone(),
            elapsed: Duration::ZERO,
        });
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
                self.pending = None;
                self.selected_choice = 0;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.active = None;
        self.pending = None;
        self.selected_choice = 0;
        self.accepted = None;
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
            self.pending = None;
        }
    }

    #[must_use]
    pub(crate) fn text<'a>(&self, registry: &'a ContentRegistry) -> Option<&'a str> {
        let active = self.active?;
        registry
            .npc_dialogue_presentation_by_id(active.npc_content_id)?
            .beat(DialogueBeatIndex::from_raw(active.beat_index))
            .map(|beat| beat.display_text.as_str())
    }

    #[must_use]
    pub(crate) fn presentation_cue<'a>(
        &self,
        registry: &'a ContentRegistry,
    ) -> Option<DialoguePresentationCue<'a>> {
        let active = self.active?;
        let line = registry
            .npc_dialogue_presentation_by_id(active.npc_content_id)?
            .beat(DialogueBeatIndex::from_raw(active.beat_index))?
            .lines
            .get(active.line_index as usize)?;
        Some(DialoguePresentationCue {
            target: active.target,
            revision: self.presentation_revision,
            animation: line.animation.as_deref(),
        })
    }

    #[must_use]
    pub(crate) fn choices<'a>(
        &self,
        registry: &'a ContentRegistry,
    ) -> Option<&'a [purgatory_content::DialoguePresentationChoice]> {
        if self.accepted.is_some() {
            return None;
        }
        let active = self.active?;
        let choices = &registry
            .npc_dialogue_presentation_by_id(active.npc_content_id)?
            .beat(DialogueBeatIndex::from_raw(active.beat_index))?
            .choices;
        (!choices.is_empty()).then_some(choices.as_slice())
    }

    #[must_use]
    pub(crate) fn selected_choice(&self) -> usize {
        self.selected_choice
    }

    pub(crate) fn select_choice(&mut self, index: usize, registry: &ContentRegistry) -> bool {
        let Some(choices) = self.choices(registry) else {
            return false;
        };
        if index >= choices.len() {
            return false;
        }
        self.selected_choice = index;
        true
    }

    pub(crate) fn select_next(&mut self, registry: &ContentRegistry, direction: i32) -> bool {
        let Some(len) = self.choices(registry).map(<[_]>::len) else {
            return false;
        };
        self.selected_choice = if direction < 0 {
            (self.selected_choice + len - 1) % len
        } else {
            (self.selected_choice + 1) % len
        };
        true
    }

    #[must_use]
    pub(crate) fn choice_request(&self, registry: &ContentRegistry) -> Option<DialogueChoose> {
        let active = self.active?;
        let choices = self.choices(registry)?;
        let choice_index = u32::try_from(self.selected_choice).ok()?;
        choices.get(self.selected_choice)?;
        Some(DialogueChoose {
            session_id: active.session_id,
            beat_index: active.beat_index,
            choice_index,
        })
    }

    #[must_use]
    pub(crate) fn player_text(&self) -> Option<&str> {
        self.accepted
            .as_ref()
            .map(|accepted| accepted.text.as_str())
    }

    pub(crate) fn tick(&mut self, elapsed: Duration) {
        let Some(accepted) = self.accepted.as_mut() else {
            return;
        };
        accepted.elapsed = accepted.elapsed.saturating_add(elapsed);
        if accepted.elapsed < ACCEPTED_CHOICE_VISIBLE {
            return;
        }
        self.accepted = None;
        if let Some(next) = self.pending.take() {
            self.activate_line(next);
            self.selected_choice = 0;
        }
    }

    fn activate_line(&mut self, line: ServerDialogueLine) {
        if self.active != Some(line) {
            self.presentation_revision = self.presentation_revision.wrapping_add(1);
        }
        self.active = Some(line);
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

        line.beat_index = 99;
        assert_eq!(
            runtime.apply_line(
                line,
                UIRuntimeState::Active {
                    session_id: line.session_id,
                    target: line.target,
                },
                &registry,
            ),
            Err(DialogueProjectionError::UnknownBeat)
        );

        line.beat_index = 0;
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

    #[test]
    fn choice_navigation_and_acceptance_delay_the_authoritative_continuation() {
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("content");
        let line = traveler_line();
        let interaction = UIRuntimeState::Active {
            session_id: line.session_id,
            target: line.target,
        };
        let mut runtime = DialogueRuntime::default();
        runtime.apply_line(line, interaction, &registry).unwrap();
        let first_cue = runtime.presentation_cue(&registry).unwrap();
        assert_eq!(first_cue.animation, Some("dialogue_talk"));
        assert_eq!(runtime.choices(&registry).unwrap().len(), 3);
        assert!(runtime.select_next(&registry, 1));
        assert_eq!(runtime.choice_request(&registry).unwrap().choice_index, 1);

        runtime
            .apply_choice_accepted(
                ServerDialogueChoiceAccepted {
                    session_id: 7,
                    beat_index: 0,
                    choice_index: 1,
                },
                &registry,
            )
            .unwrap();
        assert_eq!(runtime.player_text(), Some("I'm just passing through."));
        let continuation = ServerDialogueLine {
            beat_index: 2,
            ..line
        };
        runtime
            .apply_line(continuation, interaction, &registry)
            .unwrap();
        assert_eq!(runtime.active().unwrap().beat_index, 0);
        runtime.tick(Duration::from_millis(999));
        assert_eq!(runtime.active().unwrap().beat_index, 0);
        assert_eq!(runtime.presentation_cue(&registry).unwrap(), first_cue);
        runtime.tick(Duration::from_millis(1));
        assert_eq!(runtime.active().unwrap().beat_index, 2);
        assert_eq!(runtime.player_text(), None);
        let continuation_cue = runtime.presentation_cue(&registry).unwrap();
        assert!(continuation_cue.revision > first_cue.revision);
        assert_eq!(continuation_cue.animation, None);
    }

    #[test]
    fn two_clients_keep_distinct_lines_for_the_same_npc_target() {
        let registry = load_registry(&default_content_root(), LoadMode::Shared).expect("content");
        let first = traveler_line();
        let second = ServerDialogueLine {
            session_id: 8,
            beat_index: 1,
            ..first
        };
        let mut client_a = DialogueRuntime::default();
        let mut client_b = DialogueRuntime::default();
        client_a
            .apply_line(
                first,
                UIRuntimeState::Active {
                    session_id: first.session_id,
                    target: first.target,
                },
                &registry,
            )
            .unwrap();
        client_b
            .apply_line(
                second,
                UIRuntimeState::Active {
                    session_id: second.session_id,
                    target: second.target,
                },
                &registry,
            )
            .unwrap();

        assert_ne!(client_a.text(&registry), client_b.text(&registry));
        assert_eq!(
            client_a.presentation_cue(&registry).unwrap().animation,
            Some("dialogue_talk")
        );
        assert_eq!(
            client_b.presentation_cue(&registry).unwrap().animation,
            None
        );
        client_a.clear();
        assert!(client_b.is_active());
    }
}
