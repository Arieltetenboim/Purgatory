//! Authoritative per-player narrative state.
//!
//! This owner is deliberately separate from dialogue progression and item
//! state. Its shape can later be serialized with character persistence without
//! making dialogue sessions persistent.

use std::collections::{HashMap, HashSet};

use purgatory_common::ContentId;
use purgatory_content::DialogueBeatIndex;
use purgatory_simulation::EntityId;

#[derive(Default)]
struct PlayerNarrativeState {
    facts: HashMap<String, bool>,
    npcs_met: HashSet<String>,
    dialogue_heard: HashSet<(ContentId, DialogueBeatIndex)>,
}

#[derive(Default)]
pub(crate) struct NarrativeRuntime {
    by_actor: HashMap<EntityId, PlayerNarrativeState>,
}

impl NarrativeRuntime {
    pub(crate) fn initialize_actor(&mut self, actor: EntityId) {
        self.by_actor.entry(actor).or_default();
    }

    pub(crate) fn forget_actor(&mut self, actor: EntityId) {
        self.by_actor.remove(&actor);
    }

    #[must_use]
    pub(crate) fn fact(&self, actor: EntityId, fact: &str) -> bool {
        self.by_actor
            .get(&actor)
            .and_then(|state| state.facts.get(fact))
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn set_fact(&mut self, actor: EntityId, fact: &str, value: bool) {
        self.by_actor
            .entry(actor)
            .or_default()
            .facts
            .insert(fact.into(), value);
    }

    #[must_use]
    pub(crate) fn npc_met(&self, actor: EntityId, npc_authored: &str) -> bool {
        self.by_actor
            .get(&actor)
            .is_some_and(|state| state.npcs_met.contains(npc_authored))
    }

    pub(crate) fn mark_npc_met(&mut self, actor: EntityId, npc_authored: &str) {
        self.by_actor
            .entry(actor)
            .or_default()
            .npcs_met
            .insert(npc_authored.into());
    }

    #[must_use]
    pub(crate) fn dialogue_heard(
        &self,
        actor: EntityId,
        npc_content_id: ContentId,
        beat_index: DialogueBeatIndex,
    ) -> bool {
        self.by_actor
            .get(&actor)
            .is_some_and(|state| state.dialogue_heard.contains(&(npc_content_id, beat_index)))
    }

    pub(crate) fn mark_dialogue_heard(
        &mut self,
        actor: EntityId,
        npc_content_id: ContentId,
        beat_index: DialogueBeatIndex,
    ) {
        self.by_actor
            .entry(actor)
            .or_default()
            .dialogue_heard
            .insert((npc_content_id, beat_index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_states_are_independent() {
        let actor_a = EntityId::from_raw(1, 1);
        let actor_b = EntityId::from_raw(2, 1);
        let mut runtime = NarrativeRuntime::default();
        runtime.initialize_actor(actor_a);
        runtime.initialize_actor(actor_b);

        runtime.set_fact(actor_a, "fact.test", true);
        runtime.mark_npc_met(actor_a, "npc.welcome.workshop_craftsperson");
        runtime.mark_dialogue_heard(
            actor_a,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(3),
        );

        assert!(runtime.fact(actor_a, "fact.test"));
        assert!(!runtime.fact(actor_b, "fact.test"));
        assert!(runtime.npc_met(actor_a, "npc.welcome.workshop_craftsperson"));
        assert!(!runtime.npc_met(actor_b, "npc.welcome.workshop_craftsperson"));
        assert!(runtime.dialogue_heard(
            actor_a,
            ContentId::from_raw(20_001),
            DialogueBeatIndex::from_raw(3)
        ));
    }

    #[test]
    fn forgetting_actor_removes_only_its_transient_narrative_state() {
        let actor = EntityId::from_raw(1, 1);
        let mut runtime = NarrativeRuntime::default();
        runtime.initialize_actor(actor);
        runtime.mark_npc_met(actor, "npc.test.one");

        runtime.forget_actor(actor);

        assert!(!runtime.fact(actor, "fact.test"));
        assert!(!runtime.npc_met(actor, "npc.test.one"));
    }
}
