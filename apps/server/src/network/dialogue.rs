//! Per-player authoritative dialogue progression.
//!
//! This module owns dialogue state only. Interaction validity and lifetime
//! remain in `World::InteractionSession`; presentation remains client-owned.

use std::collections::{HashMap, HashSet};

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdvanceResult {
    WaitingForChoice(ActiveDialogue),
    Complete(ActiveDialogue),
    Invalid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChoiceResult {
    Continue {
        accepted: ActiveDialogue,
        choice_index: u32,
        next: ActiveDialogue,
    },
    Complete {
        accepted: ActiveDialogue,
        choice_index: u32,
    },
    Invalid,
}

#[derive(Default)]
pub(crate) struct DialogueRuntime {
    active_by_actor: HashMap<EntityId, ActiveDialogue>,
    heard_by_actor: HashSet<(EntityId, ContentId, DialogueBeatIndex)>,
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

    pub(crate) fn forget_actor(&mut self, actor: EntityId) {
        self.active_by_actor.remove(&actor);
        self.heard_by_actor
            .retain(|(heard_actor, _, _)| *heard_actor != actor);
    }

    pub(crate) fn advance(
        &mut self,
        actor: EntityId,
        session_id: u32,
        definition: &NpcDialogueDefinition,
    ) -> AdvanceResult {
        let Some(active) = self.active_by_actor.get(&actor).copied() else {
            return AdvanceResult::Invalid;
        };
        if active.session_id != session_id || active.npc_content_id != definition.content_id {
            return AdvanceResult::Invalid;
        }
        let Some(beat) = definition.beat(active.beat_index) else {
            return AdvanceResult::Invalid;
        };
        if beat.choices.is_empty() {
            self.heard_by_actor
                .insert((actor, definition.content_id, active.beat_index));
            AdvanceResult::Complete(active)
        } else {
            // A Beat with choices waits for an explicit, validated choice.
            AdvanceResult::WaitingForChoice(active)
        }
    }

    pub(crate) fn choose(
        &mut self,
        actor: EntityId,
        session_id: u32,
        beat_index: DialogueBeatIndex,
        choice_index: u32,
        definition: &NpcDialogueDefinition,
    ) -> ChoiceResult {
        let Some(active) = self.active_by_actor.get(&actor).copied() else {
            return ChoiceResult::Invalid;
        };
        if active.session_id != session_id
            || active.npc_content_id != definition.content_id
            || active.beat_index != beat_index
        {
            return ChoiceResult::Invalid;
        }
        let Some(choice) = definition
            .beat(active.beat_index)
            .and_then(|beat| beat.choices.get(choice_index as usize))
        else {
            return ChoiceResult::Invalid;
        };
        self.heard_by_actor
            .insert((actor, definition.content_id, active.beat_index));
        let Some(next_index) = choice.next else {
            self.active_by_actor.remove(&actor);
            return ChoiceResult::Complete {
                accepted: active,
                choice_index,
            };
        };
        let next = ActiveDialogue {
            beat_index: next_index,
            ..active
        };
        self.active_by_actor.insert(actor, next);
        ChoiceResult::Continue {
            accepted: active,
            choice_index,
            next,
        }
    }

    #[must_use]
    pub(crate) fn has_heard(
        &self,
        actor: EntityId,
        npc_content_id: ContentId,
        beat_index: DialogueBeatIndex,
    ) -> bool {
        self.heard_by_actor
            .contains(&(actor, npc_content_id, beat_index))
    }
}

/// Current authoritative state adapter for the proven NPC Lab selector.
/// Narrative facts and NPC Met remain false until their N10e owner exists.
/// N10d owns transient per-player Heard completion for selection semantics.
pub(crate) struct RuntimeConditions<'a> {
    pub world: &'a World,
    pub registry: &'a ContentRegistry,
    pub actor: EntityId,
    pub dialogues: &'a DialogueRuntime,
}

impl DialogueConditionState for RuntimeConditions<'_> {
    fn fact(&self, _fact: &str) -> bool {
        false
    }

    fn npc_met(&self, _npc_authored: &str) -> bool {
        false
    }

    fn dialogue_heard(&self, npc_authored: &str, beat_id: &str) -> bool {
        let Some(definition) = self.registry.npc_dialogue(npc_authored) else {
            return false;
        };
        let Some(index) = definition.beats.iter().position(|beat| beat.id == beat_id) else {
            return false;
        };
        let Ok(index) = u32::try_from(index) else {
            return false;
        };
        self.dialogues.has_heard(
            self.actor,
            definition.content_id,
            DialogueBeatIndex::from_raw(index),
        )
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
    fn beat_progression_is_session_scoped_and_stops_before_choices() {
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
            AdvanceResult::WaitingForChoice(_)
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
            AdvanceResult::Complete(_)
        ));
        assert_eq!(runtime.active(actor_b).unwrap().session_id, 8);
    }

    #[test]
    fn choice_validates_current_beat_marks_heard_and_follows_authored_next() {
        let actor = EntityId::from_raw(1, 1);
        let target = EntityId::from_raw(2, 1);
        let mut definition = definition(true);
        definition.beats.push(DialogueBeat {
            id: "next".into(),
            selection_role: DialogueSelectionRole::Continuation,
            priority: 0,
            pool: DialoguePool::Mandatory,
            conditions: vec![],
            lines: vec![DialogueLine {
                text: "next".into(),
            }],
            choices: vec![],
        });
        definition.beats[0].choices[0].next = Some(DialogueBeatIndex::from_raw(1));
        let mut runtime = DialogueRuntime::default();
        runtime.begin(
            actor,
            7,
            target,
            definition.content_id,
            DialogueBeatIndex::from_raw(0),
        );
        assert_eq!(
            runtime.choose(actor, 7, DialogueBeatIndex::from_raw(9), 0, &definition),
            ChoiceResult::Invalid
        );
        let result = runtime.choose(actor, 7, DialogueBeatIndex::from_raw(0), 0, &definition);
        assert!(matches!(
            result,
            ChoiceResult::Continue {
                next: ActiveDialogue { beat_index, .. },
                ..
            } if beat_index == DialogueBeatIndex::from_raw(1)
        ));
        assert!(runtime.has_heard(actor, definition.content_id, DialogueBeatIndex::from_raw(0)));
    }
}
