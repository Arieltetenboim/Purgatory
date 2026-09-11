//! Per-player authoritative dialogue progression.
//!
//! This module owns dialogue state only. Interaction validity and lifetime
//! remain in `World::InteractionSession`; presentation remains client-owned.

use std::collections::HashMap;

use purgatory_common::ContentId;
use purgatory_content::{
    ContentRegistry, DialogueAction, DialogueBeatIndex, DialogueConditionState,
    NpcDialogueDefinition,
};
use purgatory_simulation::{EntityId, EquipmentSlot, World};

use super::narrative::NarrativeRuntime;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChoicePlan {
    pub accepted: ActiveDialogue,
    pub choice_index: u32,
    pub next: Option<ActiveDialogue>,
    pub actions: Vec<DialogueAction>,
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
    }

    pub(crate) fn advance(
        &self,
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
            AdvanceResult::Complete(active)
        } else {
            // A Beat with choices waits for an explicit, validated choice.
            AdvanceResult::WaitingForChoice(active)
        }
    }

    pub(crate) fn plan_choice(
        &self,
        actor: EntityId,
        session_id: u32,
        beat_index: DialogueBeatIndex,
        choice_index: u32,
        definition: &NpcDialogueDefinition,
    ) -> Option<ChoicePlan> {
        let active = self.active_by_actor.get(&actor).copied()?;
        if active.session_id != session_id
            || active.npc_content_id != definition.content_id
            || active.beat_index != beat_index
        {
            return None;
        }
        let choice = definition
            .beat(active.beat_index)
            .and_then(|beat| beat.choices.get(choice_index as usize))?;
        Some(ChoicePlan {
            accepted: active,
            choice_index,
            next: choice.next.map(|beat_index| ActiveDialogue {
                beat_index,
                ..active
            }),
            actions: choice.actions.clone(),
        })
    }

    pub(crate) fn commit_choice(&mut self, plan: ChoicePlan) -> ChoiceResult {
        if self.active(plan.accepted.actor) != Some(plan.accepted) {
            return ChoiceResult::Invalid;
        }
        let Some(next) = plan.next else {
            self.active_by_actor.remove(&plan.accepted.actor);
            return ChoiceResult::Complete {
                accepted: plan.accepted,
                choice_index: plan.choice_index,
            };
        };
        self.active_by_actor.insert(plan.accepted.actor, next);
        ChoiceResult::Continue {
            accepted: plan.accepted,
            choice_index: plan.choice_index,
            next,
        }
    }
}

/// Current authoritative state adapter for the proven NPC Lab selector.
/// This adapter reads each condition from its existing authoritative owner.
pub(crate) struct RuntimeConditions<'a> {
    pub world: &'a World,
    pub registry: &'a ContentRegistry,
    pub actor: EntityId,
    pub narrative: &'a NarrativeRuntime,
}

impl DialogueConditionState for RuntimeConditions<'_> {
    fn fact(&self, fact: &str) -> bool {
        self.narrative.fact(self.actor, fact)
    }

    fn npc_met(&self, npc_authored: &str) -> bool {
        self.narrative.npc_met(self.actor, npc_authored)
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
        self.narrative.dialogue_heard(
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
        DialogueBeat, DialogueLine, DialoguePool, DialogueSelectionRole, LoadMode,
        NpcDialogueDefinition, default_content_root, load_registry,
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
        let mut definition = definition(false);
        definition.beats.push(DialogueBeat {
            id: "other_entry".into(),
            selection_role: DialogueSelectionRole::Entry,
            priority: 0,
            pool: DialoguePool::Mandatory,
            conditions: vec![],
            lines: vec![DialogueLine {
                text: "different player state".into(),
            }],
            choices: vec![],
        });
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
            DialogueBeatIndex::from_raw(1),
        );

        assert!(matches!(
            runtime.advance(actor_a, 7, &definition),
            AdvanceResult::Complete(_)
        ));
        assert_eq!(runtime.active(actor_b).unwrap().session_id, 8);
        assert_eq!(
            runtime.active(actor_b).unwrap().beat_index,
            DialogueBeatIndex::from_raw(1)
        );
    }

    #[test]
    fn choice_is_planned_without_mutation_then_committed_to_authored_next() {
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
        assert!(
            runtime
                .plan_choice(actor, 7, DialogueBeatIndex::from_raw(9), 0, &definition)
                .is_none()
        );
        let plan = runtime
            .plan_choice(actor, 7, DialogueBeatIndex::from_raw(0), 0, &definition)
            .expect("valid choice plan");
        assert_eq!(
            runtime.active(actor).map(|active| active.beat_index),
            Some(DialogueBeatIndex::from_raw(0)),
            "planning must not consume dialogue state before actions succeed"
        );
        let result = runtime.commit_choice(plan);
        assert!(matches!(
            result,
            ChoiceResult::Continue {
                next: ActiveDialogue { beat_index, .. },
                ..
            } if beat_index == DialogueBeatIndex::from_raw(1)
        ));
    }

    #[test]
    fn runtime_conditions_read_narrative_inventory_and_equipment_owners() {
        let registry = load_registry(&default_content_root(), LoadMode::Full).unwrap();
        let mut world = World::dev_stage();
        let actor = world.player_id().unwrap();
        let mut narrative = NarrativeRuntime::default();
        narrative.initialize_actor(actor);
        narrative.set_fact(actor, "fact.test.enabled", true);
        narrative.mark_npc_met(actor, "npc.welcome.gate_watchman");
        narrative.mark_dialogue_heard(
            actor,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(0),
        );
        let sword = registry.item("equipment.debug.practice_sword").unwrap();
        let (item, _) = world
            .grant_inventory_item(actor, sword.content_id, 1, sword.stack_limit)
            .unwrap();

        let conditions = RuntimeConditions {
            world: &world,
            registry: &registry,
            actor,
            narrative: &narrative,
        };
        assert!(conditions.fact("fact.test.enabled"));
        assert!(conditions.npc_met("npc.welcome.gate_watchman"));
        assert!(conditions.dialogue_heard("npc.welcome.traveler_stayed", "intro"));
        assert!(conditions.item_owned("equipment.debug.practice_sword"));
        assert!(!conditions.item_equipped("equipment.debug.practice_sword"));

        world
            .equip_item(actor, item, EquipmentSlot::Weapon)
            .unwrap();
        let conditions = RuntimeConditions {
            world: &world,
            registry: &registry,
            actor,
            narrative: &narrative,
        };
        assert!(!conditions.item_owned("equipment.debug.practice_sword"));
        assert!(conditions.item_equipped("equipment.debug.practice_sword"));
    }
}
