//! Authoritative dialogue action routing.
//!
//! This integration boundary resolves authored references, validates the whole
//! inventory mutation sequence, then delegates each mutation to its domain
//! owner in authored order.

use purgatory_common::ContentId;
use purgatory_content::{ContentRegistry, DialogueAction};
use purgatory_simulation::{EntityId, INVENTORY_CAPACITY, ItemRuntimeError, World};

use super::narrative::NarrativeRuntime;

#[derive(Clone, Debug, Eq, PartialEq)]
enum ResolvedAction {
    SetFact {
        fact: String,
        value: bool,
    },
    MarkNpcMet {
        npc_authored: String,
    },
    GiveItem {
        definition: ContentId,
        quantity: u32,
        stack_limit: u32,
    },
    RemoveItem {
        definition: ContentId,
        quantity: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DialogueActionError {
    UnknownItem(String),
    Inventory(ItemRuntimeError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DialogueActionOutcome {
    pub inventory_changed: bool,
}

pub(crate) fn execute_dialogue_actions(
    actions: &[DialogueAction],
    actor: EntityId,
    registry: &ContentRegistry,
    world: &mut World,
    narrative: &mut NarrativeRuntime,
) -> Result<DialogueActionOutcome, DialogueActionError> {
    let resolved = resolve(actions, registry)?;
    preflight_inventory(&resolved, actor, world)?;

    let mut outcome = DialogueActionOutcome::default();
    for action in resolved {
        match action {
            ResolvedAction::SetFact { fact, value } => {
                narrative.set_fact(actor, &fact, value);
            }
            ResolvedAction::MarkNpcMet { npc_authored } => {
                narrative.mark_npc_met(actor, &npc_authored);
            }
            ResolvedAction::GiveItem {
                definition,
                quantity,
                stack_limit,
            } => {
                world
                    .grant_inventory_item(actor, definition, quantity, stack_limit)
                    .map_err(DialogueActionError::Inventory)?;
                outcome.inventory_changed = true;
            }
            ResolvedAction::RemoveItem {
                definition,
                quantity,
            } => {
                world
                    .remove_inventory_definition(actor, definition, quantity)
                    .map_err(DialogueActionError::Inventory)?;
                outcome.inventory_changed = true;
            }
        }
    }
    Ok(outcome)
}

fn resolve(
    actions: &[DialogueAction],
    registry: &ContentRegistry,
) -> Result<Vec<ResolvedAction>, DialogueActionError> {
    actions
        .iter()
        .map(|action| match action {
            DialogueAction::SetFact { fact, value } => Ok(ResolvedAction::SetFact {
                fact: fact.clone(),
                value: *value,
            }),
            DialogueAction::MarkNpcMet { npc_authored } => Ok(ResolvedAction::MarkNpcMet {
                npc_authored: npc_authored.clone(),
            }),
            DialogueAction::GiveItem {
                item_authored,
                quantity,
            } => {
                let item = registry
                    .item(item_authored)
                    .ok_or_else(|| DialogueActionError::UnknownItem(item_authored.clone()))?;
                Ok(ResolvedAction::GiveItem {
                    definition: item.content_id,
                    quantity: *quantity,
                    stack_limit: item.stack_limit,
                })
            }
            DialogueAction::RemoveItem {
                item_authored,
                quantity,
            } => {
                let item = registry
                    .item(item_authored)
                    .ok_or_else(|| DialogueActionError::UnknownItem(item_authored.clone()))?;
                Ok(ResolvedAction::RemoveItem {
                    definition: item.content_id,
                    quantity: *quantity,
                })
            }
        })
        .collect()
}

fn preflight_inventory(
    actions: &[ResolvedAction],
    actor: EntityId,
    world: &World,
) -> Result<(), DialogueActionError> {
    let mut stacks: Vec<(ContentId, u32)> = world
        .inventory_snapshot(actor)
        .into_iter()
        .map(|(_, _, record)| (record.definition, record.quantity))
        .collect();

    for action in actions {
        match *action {
            ResolvedAction::GiveItem {
                definition,
                quantity,
                stack_limit,
            } => {
                if stack_limit == 0 || quantity == 0 || quantity > stack_limit {
                    return Err(DialogueActionError::Inventory(
                        ItemRuntimeError::InvalidQuantity {
                            quantity,
                            stack_limit,
                        },
                    ));
                }
                if stacks.len() >= INVENTORY_CAPACITY {
                    return Err(DialogueActionError::Inventory(
                        ItemRuntimeError::InventoryFull(actor),
                    ));
                }
                stacks.push((definition, quantity));
            }
            ResolvedAction::RemoveItem {
                definition,
                quantity,
            } => {
                let available = stacks
                    .iter()
                    .filter(|(candidate, _)| *candidate == definition)
                    .fold(0u32, |total, (_, amount)| total.saturating_add(*amount));
                if quantity == 0 || available < quantity {
                    return Err(DialogueActionError::Inventory(
                        ItemRuntimeError::InsufficientInventoryQuantity {
                            owner: actor,
                            definition,
                            requested: quantity,
                            available,
                        },
                    ));
                }
                let mut remaining = quantity;
                for (candidate, amount) in &mut stacks {
                    if *candidate != definition || remaining == 0 {
                        continue;
                    }
                    let removed = remaining.min(*amount);
                    *amount -= removed;
                    remaining -= removed;
                }
                stacks.retain(|(_, amount)| *amount > 0);
            }
            ResolvedAction::SetFact { .. } | ResolvedAction::MarkNpcMet { .. } => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use purgatory_content::{LoadMode, default_content_root, load_registry};

    #[test]
    fn action_batch_routes_narrative_and_inventory_mutations() {
        let registry = load_registry(&default_content_root(), LoadMode::Full).unwrap();
        let mut world = World::dev_stage();
        let actor = world.player_id().unwrap();
        let mut narrative = NarrativeRuntime::default();
        narrative.initialize_actor(actor);
        let actions = vec![
            DialogueAction::GiveItem {
                item_authored: "item.package".into(),
                quantity: 1,
            },
            DialogueAction::SetFact {
                fact: "welcome.workshop.package_at_inn".into(),
                value: false,
            },
            DialogueAction::MarkNpcMet {
                npc_authored: "npc.welcome.workshop_craftsperson".into(),
            },
        ];

        let outcome =
            execute_dialogue_actions(&actions, actor, &registry, &mut world, &mut narrative)
                .unwrap();

        assert!(outcome.inventory_changed);
        assert_eq!(world.inventory_count(actor), 1);
        assert!(!narrative.fact(actor, "welcome.workshop.package_at_inn"));
        assert!(narrative.npc_met(actor, "npc.welcome.workshop_craftsperson"));
    }

    #[test]
    fn failed_preflight_applies_no_earlier_narrative_action() {
        let registry = load_registry(&default_content_root(), LoadMode::Full).unwrap();
        let mut world = World::dev_stage();
        let actor = world.player_id().unwrap();
        let mut narrative = NarrativeRuntime::default();
        let actions = vec![
            DialogueAction::SetFact {
                fact: "fact.must.not.change".into(),
                value: true,
            },
            DialogueAction::RemoveItem {
                item_authored: "item.package".into(),
                quantity: 1,
            },
        ];

        assert!(
            execute_dialogue_actions(&actions, actor, &registry, &mut world, &mut narrative,)
                .is_err()
        );
        assert!(!narrative.fact(actor, "fact.must.not.change"));
    }
}
