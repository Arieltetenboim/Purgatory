//! Per-player authoritative dialogue progression.
//!
//! This module owns dialogue state only. Interaction validity and lifetime
//! remain in `World::InteractionSession`; presentation remains client-owned.

use std::collections::HashMap;

use purgatory_common::ContentId;
use purgatory_content::{
    ContentRegistry, DialogueBeatIndex, DialogueConditionState, NpcDialogueDefinition,
};
use purgatory_simulation::{EntityId, EquipmentSlot, World};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ActiveDialogue {
    pub actor: EntityId,
    pub session_id: u32,
    pub target: EntityId,
    pub npc_content_id: ContentId,
    pub beat_index: DialogueBeatIndex,
    pub line_index: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdvanceResult {
    Line(ActiveDialogue),
    WaitingForChoice(ActiveDialogue),
    Complete(ActiveDialogue),
    Invalid,
}

#[derive(Default)]
pub(crate) struct DialogueRuntime {
    active_by_actor: HashMap<EntityId, ActiveDialogue>,
}

impl DialogueRuntime {
    pub(crate) fn begin(
        &mut self,
        actor: EntityId,
        session_id: u32,
        target: EntityId,
        npc_content_id: ContentId,
        beat_index: DialogueBeatIndex,
    ) -> ActiveDialogue {
        let active = ActiveDialogue {
            actor,
            session_id,
            target,
            npc_content_id,
            beat_index,
            line_index: 0,
        };
        self.active_by_actor.insert(actor, active);
        active
    }

    #[must_use]
    pub(crate) fn active(&self, actor: EntityId) -> Option<ActiveDialogue> {
        self.active_by_actor.get(&actor).copied()
    }

    pub(crate) fn remove(&mut self, actor: EntityId) -> Option<ActiveDialogue> {
        self.active_by_actor.remove(&actor)
    }

    pub(crate) fn advance(
        &mut self,
        actor: EntityId,
        session_id: u32,
        definition: &NpcDialogueDefinition,
    ) -> AdvanceResult {
        let Some(active) = self.active_by_actor.get_mut(&actor) else {
            return AdvanceResult::Invalid;
        };
        if active.session_id != session_id || active.npc_content_id != definition.content_id {
            return AdvanceResult::Invalid;
        }
        let Some(beat) = definition.beat(active.beat_index) else {
            return AdvanceResult::Invalid;
        };
        let Some(next) = active.line_index.checked_add(1) else {
            return AdvanceResult::Invalid;
        };
        if usize::try_from(next).is_ok_and(|index| index < beat.lines.len()) {
            active.line_index = next;
            return AdvanceResult::Line(*active);
        }
        if beat.choices.is_empty() {
            AdvanceResult::Complete(*active)
        } else {
            // Choice selection is deliberately owned by N10d. N10c keeps the
            // final line visible and does not invent an implicit choice.
            AdvanceResult::WaitingForChoice(*active)
        }
    }
}

/// Current authoritative state adapter for the proven NPC Lab selector.
/// Narrative facts/Met/Heard remain false until their N10e owner exists.
pub(crate) struct RuntimeConditions<'a> {
    pub world: &'a World,
    pub registry: &'a ContentRegistry,
    pub actor: EntityId,
}

impl DialogueConditionState for RuntimeConditions<'_> {
    fn fact(&self, _fact: &str) -> bool {
        false
    }

    fn npc_met(&self, _npc_authored: &str) -> bool {
        false
    }

    fn dialogue_heard(&self, _npc_authored: &str, _beat_id: &str) -> bool {
        false
    }

    fn item_owned(&self, item_authored: &str) -> bool {
        let Some(content_id) = self
            .registry
            .item(item_authored)
            .map(|item| item.content_id)
        else {
            return false;
        };
        self.world
            .inventory_snapshot(self.actor)
            .iter()
            .any(|(_, _, record)| record.definition == content_id)
    }

    fn item_equipped(&self, item_authored: &str) -> bool {
        let Some(content_id) = self
            .registry
            .item(item_authored)
            .map(|item| item.content_id)
        else {
            return false;
        };
        EquipmentSlot::ALL
            .into_iter()
            .any(|slot| self.world.equipment_slot(self.actor, slot) == Some(content_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_content::{
        DialogueBeat, DialogueLine, DialoguePool, DialogueSelectionRole, NpcDialogueDefinition,
    };

    fn definition(choices: bool) -> NpcDialogueDefinition {
        NpcDialogueDefinition {
            content_id: ContentId::from_raw(20_001),
            authored_id: "npc.test.dialogue".into(),
            beats: vec![DialogueBeat {
                id: "entry".into(),
                selection_role: DialogueSelectionRole::Entry,
                priority: 0,
                pool: DialoguePool::Mandatory,
                conditions: vec![],
                lines: vec![
                    DialogueLine { text: "a".into() },
                    DialogueLine { text: "b".into() },
                ],
                choices: if choices {
                    vec![purgatory_content::DialogueChoice {
                        id: "later".into(),
                        text: "later".into(),
                        next: None,
                        actions: vec![],
                    }]
                } else {
                    vec![]
                },
            }],
        }
    }

    #[test]
    fn progression_is_session_scoped_and_stops_before_choices() {
        let actor = EntityId::from_raw(1, 1);
        let target = EntityId::from_raw(2, 1);
        let mut runtime = DialogueRuntime::default();
        runtime.begin(
            actor,
            7,
            target,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(0),
        );

        assert_eq!(
            runtime.advance(actor, 8, &definition(true)),
            AdvanceResult::Invalid
        );
        assert!(matches!(
            runtime.advance(actor, 7, &definition(true)),
            AdvanceResult::Line(ActiveDialogue { line_index: 1, .. })
        ));
        assert!(matches!(
            runtime.advance(actor, 7, &definition(true)),
            AdvanceResult::WaitingForChoice(ActiveDialogue { line_index: 1, .. })
        ));
    }

    #[test]
    fn final_line_without_choices_completes_without_consuming_state() {
        let actor = EntityId::from_raw(1, 1);
        let target = EntityId::from_raw(2, 1);
        let mut runtime = DialogueRuntime::default();
        runtime.begin(
            actor,
            7,
            target,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(0),
        );
        assert!(matches!(
            runtime.advance(actor, 7, &definition(false)),
            AdvanceResult::Line(_)
        ));
        assert!(matches!(
            runtime.advance(actor, 7, &definition(false)),
            AdvanceResult::Complete(_)
        ));
        assert!(runtime.active(actor).is_some());
    }

    #[test]
    fn two_players_progress_independently_at_the_same_npc() {
        let actor_a = EntityId::from_raw(1, 1);
        let actor_b = EntityId::from_raw(2, 1);
        let target = EntityId::from_raw(3, 1);
        let mut runtime = DialogueRuntime::default();
        runtime.begin(
            actor_a,
            7,
            target,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(0),
        );
        runtime.begin(
            actor_b,
            8,
            target,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(0),
        );

        assert!(matches!(
            runtime.advance(actor_a, 7, &definition(false)),
            AdvanceResult::Line(ActiveDialogue { line_index: 1, .. })
        ));
        assert_eq!(runtime.active(actor_b).unwrap().line_index, 0);
        assert_eq!(runtime.active(actor_b).unwrap().session_id, 8);
    }
}
