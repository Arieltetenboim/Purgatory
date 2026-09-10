//! Server-authoritative dialogue content projected from NPC Lab authoring JSON.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use purgatory_common::{ContentId, ContentKind, validate_authored_id};
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::{ContentError, ValidationIssue};

pub const NPC_DIALOGUE_SCHEMA_VERSION: u32 = 1;

/// Validated dialogue for one authored NPC. Presentation cues are deliberately absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpcDialogueDefinition {
    pub content_id: ContentId,
    pub authored_id: String,
    pub beats: Vec<DialogueBeat>,
}

impl NpcDialogueDefinition {
    #[must_use]
    pub fn beat(&self, index: DialogueBeatIndex) -> Option<&DialogueBeat> {
        self.beats.get(index.as_usize())
    }
}

/// Compact resolved index into an NPC definition's authored beat order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DialogueBeatIndex(u32);

impl DialogueBeatIndex {
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueBeat {
    pub id: String,
    pub selection_role: DialogueSelectionRole,
    pub priority: i32,
    pub pool: DialoguePool,
    pub conditions: Vec<DialogueCondition>,
    pub lines: Vec<DialogueLine>,
    pub choices: Vec<DialogueChoice>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogueSelectionRole {
    Entry,
    Continuation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialoguePool {
    Mandatory,
    Once,
    Repeatable,
    Rare,
    Lore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DialogueCondition {
    Fact {
        fact: String,
        equals: bool,
    },
    NpcMet {
        npc_authored: String,
        equals: bool,
    },
    DialogueHeard {
        npc_authored: String,
        beat_id: String,
        equals: bool,
    },
    ItemOwned {
        item_authored: String,
        equals: bool,
    },
    ItemEquipped {
        item_authored: String,
        equals: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueLine {
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueChoice {
    pub id: String,
    pub text: String,
    pub next: Option<DialogueBeatIndex>,
    pub actions: Vec<DialogueAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DialogueAction {
    SetFact {
        fact: String,
        value: bool,
    },
    MarkNpcMet {
        npc_authored: String,
    },
    GiveItem {
        item_authored: String,
        quantity: u32,
    },
    RemoveItem {
        item_authored: String,
        quantity: u32,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawNpcDialogue {
    schema_version: u32,
    id: String,
    #[serde(default, rename = "design")]
    _design: Option<IgnoredAny>,
    #[serde(default, rename = "relationships")]
    _relationships: Option<IgnoredAny>,
    interaction: RawInteraction,
    #[serde(default, rename = "notes")]
    _notes: Option<IgnoredAny>,
}

impl RawNpcDialogue {
    #[must_use]
    pub(crate) fn authored_id(&self) -> &str {
        &self.id
    }

    pub(crate) fn beat_ids(&self) -> HashSet<String> {
        self.interaction
            .beats
            .iter()
            .map(|beat| beat.id.clone())
            .collect()
    }

    pub(crate) fn validate_npc_references(
        &self,
        path: &Path,
        npc_beats: &HashMap<String, HashSet<String>>,
    ) -> Result<(), ContentError> {
        for (beat_index, beat) in self.interaction.beats.iter().enumerate() {
            for (condition_index, condition) in beat.conditions.iter().enumerate() {
                let field =
                    format!("interaction.beats[{beat_index}].conditions[{condition_index}]");
                if let Some(target) = condition.npc_met.as_deref() {
                    require_npc(
                        path,
                        &self.id,
                        &format!("{field}.npc_met"),
                        target,
                        npc_beats,
                    )?;
                }
                if let Some(reference) = condition.dialogue_heard.as_ref() {
                    require_npc(
                        path,
                        &self.id,
                        &format!("{field}.dialogue_heard.npc"),
                        &reference.npc,
                        npc_beats,
                    )?;
                    if !npc_beats[&reference.npc].contains(&reference.beat) {
                        return Err(issue(
                            path,
                            &self.id,
                            format!("{field}.dialogue_heard.beat"),
                            format!(
                                "references missing beat '{}' on NPC '{}'",
                                reference.beat, reference.npc
                            ),
                        ));
                    }
                }
            }
            for (choice_index, choice) in beat.choices.iter().enumerate() {
                for (action_index, action) in choice.actions.iter().enumerate() {
                    if let Some(target) = action.mark_npc_met.as_deref() {
                        require_npc(
                            path,
                            &self.id,
                            &format!(
                                "interaction.beats[{beat_index}].choices[{choice_index}].actions[{action_index}].mark_npc_met"
                            ),
                            target,
                            npc_beats,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn into_definition(
        self,
        path: &Path,
        content_id: Option<ContentId>,
    ) -> Result<Option<NpcDialogueDefinition>, ContentError> {
        if self.schema_version != NPC_DIALOGUE_SCHEMA_VERSION {
            return Err(issue(
                path,
                &self.id,
                "schema_version",
                format!(
                    "unsupported NPC dialogue schema version {} (want {NPC_DIALOGUE_SCHEMA_VERSION})",
                    self.schema_version
                ),
            ));
        }
        validate_reference(path, &self.id, "id", &self.id, "npc.")?;
        if content_id.is_some_and(|content_id| content_id.kind() != Some(ContentKind::Npc)) {
            return Err(issue(
                path,
                &self.id,
                "id",
                "allocated ContentId is not in the NPC block",
            ));
        }

        let mut beat_indices = HashMap::new();
        for (index, beat) in self.interaction.beats.iter().enumerate() {
            validate_local_id(path, &self.id, &format!("beats[{index}].id"), &beat.id)?;
            let index = u32::try_from(index).map_err(|_| {
                issue(
                    path,
                    &self.id,
                    "interaction.beats",
                    "too many dialogue beats",
                )
            })?;
            if beat_indices
                .insert(beat.id.clone(), DialogueBeatIndex::from_raw(index))
                .is_some()
            {
                return Err(issue(
                    path,
                    &self.id,
                    "interaction.beats.id",
                    format!("duplicate beat id '{}'", beat.id),
                ));
            }
        }

        let beats = self
            .interaction
            .beats
            .into_iter()
            .enumerate()
            .map(|(index, beat)| beat.into_definition(path, &self.id, index, &beat_indices))
            .collect::<Result<Vec<_>, _>>()?;

        let Some(content_id) = content_id else {
            return Ok(None);
        };
        Ok(Some(NpcDialogueDefinition {
            content_id,
            authored_id: self.id,
            beats,
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInteraction {
    beats: Vec<RawBeat>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBeat {
    id: String,
    #[serde(default, rename = "title")]
    _title: Option<IgnoredAny>,
    priority: i32,
    entry: bool,
    #[serde(default)]
    pool: Option<String>,
    #[serde(default)]
    conditions: Vec<RawCondition>,
    lines: Vec<RawLine>,
    #[serde(default)]
    choices: Vec<RawChoice>,
    #[serde(default, rename = "notes")]
    _notes: Option<IgnoredAny>,
}

impl RawBeat {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        beat_index: usize,
        beat_indices: &HashMap<String, DialogueBeatIndex>,
    ) -> Result<DialogueBeat, ContentError> {
        let pool = match self.pool.as_deref().unwrap_or("mandatory") {
            "mandatory" => DialoguePool::Mandatory,
            "once" => DialoguePool::Once,
            "repeatable" => DialoguePool::Repeatable,
            "rare" => DialoguePool::Rare,
            "lore" => DialoguePool::Lore,
            other => {
                return Err(issue(
                    path,
                    npc,
                    format!("interaction.beats[{beat_index}].pool"),
                    format!("unsupported dialogue pool '{other}'"),
                ));
            }
        };

        if self.lines.is_empty() {
            return Err(issue(
                path,
                npc,
                format!("interaction.beats[{beat_index}].lines"),
                "dialogue beat must contain at least one line",
            ));
        }

        let conditions = self
            .conditions
            .into_iter()
            .enumerate()
            .map(|(index, condition)| {
                condition.into_definition(
                    path,
                    npc,
                    beat_indices,
                    &format!("interaction.beats[{beat_index}].conditions[{index}]"),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let lines = self
            .lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| line.into_definition(path, npc, beat_index, index))
            .collect::<Result<Vec<_>, _>>()?;

        let mut choice_ids = HashSet::new();
        let choices = self
            .choices
            .into_iter()
            .enumerate()
            .map(|(index, choice)| {
                let field = format!("interaction.beats[{beat_index}].choices[{index}]");
                validate_local_id(path, npc, &format!("{field}.id"), &choice.id)?;
                if !choice_ids.insert(choice.id.clone()) {
                    return Err(issue(
                        path,
                        npc,
                        format!("{field}.id"),
                        format!("duplicate choice id '{}'", choice.id),
                    ));
                }
                choice.into_definition(path, npc, &field, beat_indices)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DialogueBeat {
            id: self.id,
            selection_role: if self.entry {
                DialogueSelectionRole::Entry
            } else {
                DialogueSelectionRole::Continuation
            },
            priority: self.priority,
            pool,
            conditions,
            lines,
            choices,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    text: String,
    #[serde(default, rename = "voice")]
    _voice: Option<IgnoredAny>,
    #[serde(default, rename = "animation")]
    _animation: Option<IgnoredAny>,
}

impl RawLine {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        beat_index: usize,
        line_index: usize,
    ) -> Result<DialogueLine, ContentError> {
        if self.text.trim().is_empty() {
            return Err(issue(
                path,
                npc,
                format!("interaction.beats[{beat_index}].lines[{line_index}].text"),
                "dialogue line text must not be empty",
            ));
        }
        Ok(DialogueLine { text: self.text })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawChoice {
    id: String,
    text: String,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    actions: Vec<RawAction>,
}

impl RawChoice {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        field: &str,
        beat_indices: &HashMap<String, DialogueBeatIndex>,
    ) -> Result<DialogueChoice, ContentError> {
        if self.text.trim().is_empty() {
            return Err(issue(
                path,
                npc,
                format!("{field}.text"),
                "choice text must not be empty",
            ));
        }
        let next = self
            .next
            .as_deref()
            .map(|next| {
                beat_indices.get(next).copied().ok_or_else(|| {
                    issue(
                        path,
                        npc,
                        format!("{field}.next"),
                        format!("references missing beat '{next}'"),
                    )
                })
            })
            .transpose()?;
        let actions = self
            .actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| {
                action.into_definition(path, npc, &format!("{field}.actions[{index}]"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DialogueChoice {
            id: self.id,
            text: self.text,
            next,
            actions,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCondition {
    #[serde(default)]
    fact: Option<String>,
    #[serde(default)]
    npc_met: Option<String>,
    #[serde(default)]
    dialogue_heard: Option<RawDialogueHeard>,
    #[serde(default)]
    item_owned: Option<String>,
    #[serde(default)]
    item_equipped: Option<String>,
    equals: bool,
}

impl RawCondition {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        beat_indices: &HashMap<String, DialogueBeatIndex>,
        field: &str,
    ) -> Result<DialogueCondition, ContentError> {
        let count = usize::from(self.fact.is_some())
            + usize::from(self.npc_met.is_some())
            + usize::from(self.dialogue_heard.is_some())
            + usize::from(self.item_owned.is_some())
            + usize::from(self.item_equipped.is_some());
        if count != 1 {
            return Err(issue(
                path,
                npc,
                field,
                "condition must contain exactly one supported typed check",
            ));
        }

        if let Some(fact) = self.fact {
            validate_fact(path, npc, &format!("{field}.fact"), &fact)?;
            return Ok(DialogueCondition::Fact {
                fact,
                equals: self.equals,
            });
        }
        if let Some(npc_authored) = self.npc_met {
            validate_reference(
                path,
                npc,
                &format!("{field}.npc_met"),
                &npc_authored,
                "npc.",
            )?;
            return Ok(DialogueCondition::NpcMet {
                npc_authored,
                equals: self.equals,
            });
        }
        if let Some(reference) = self.dialogue_heard {
            validate_reference(
                path,
                npc,
                &format!("{field}.dialogue_heard.npc"),
                &reference.npc,
                "npc.",
            )?;
            validate_local_id(
                path,
                npc,
                &format!("{field}.dialogue_heard.beat"),
                &reference.beat,
            )?;
            if reference.npc == npc && !beat_indices.contains_key(&reference.beat) {
                return Err(issue(
                    path,
                    npc,
                    format!("{field}.dialogue_heard.beat"),
                    format!("references missing beat '{}'", reference.beat),
                ));
            }
            return Ok(DialogueCondition::DialogueHeard {
                npc_authored: reference.npc,
                beat_id: reference.beat,
                equals: self.equals,
            });
        }
        if let Some(item_authored) = self.item_owned {
            validate_reference(
                path,
                npc,
                &format!("{field}.item_owned"),
                &item_authored,
                "item.",
            )?;
            return Ok(DialogueCondition::ItemOwned {
                item_authored,
                equals: self.equals,
            });
        }
        let item_authored = self.item_equipped.expect("exactly one condition");
        validate_reference(
            path,
            npc,
            &format!("{field}.item_equipped"),
            &item_authored,
            "item.",
        )?;
        Ok(DialogueCondition::ItemEquipped {
            item_authored,
            equals: self.equals,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDialogueHeard {
    npc: String,
    beat: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    #[serde(default)]
    set_fact: Option<RawSetFact>,
    #[serde(default)]
    mark_npc_met: Option<String>,
    #[serde(default)]
    give_item: Option<RawItemMutation>,
    #[serde(default)]
    remove_item: Option<RawItemMutation>,
}

impl RawAction {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        field: &str,
    ) -> Result<DialogueAction, ContentError> {
        let count = usize::from(self.set_fact.is_some())
            + usize::from(self.mark_npc_met.is_some())
            + usize::from(self.give_item.is_some())
            + usize::from(self.remove_item.is_some());
        if count != 1 {
            return Err(issue(
                path,
                npc,
                field,
                "action must contain exactly one supported typed operation",
            ));
        }

        if let Some(action) = self.set_fact {
            validate_fact(path, npc, &format!("{field}.set_fact.fact"), &action.fact)?;
            return Ok(DialogueAction::SetFact {
                fact: action.fact,
                value: action.value,
            });
        }
        if let Some(npc_authored) = self.mark_npc_met {
            validate_reference(
                path,
                npc,
                &format!("{field}.mark_npc_met"),
                &npc_authored,
                "npc.",
            )?;
            return Ok(DialogueAction::MarkNpcMet { npc_authored });
        }
        if let Some(action) = self.give_item {
            return action.into_definition(path, npc, field, true);
        }
        self.remove_item
            .expect("exactly one action")
            .into_definition(path, npc, field, false)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSetFact {
    fact: String,
    value: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItemMutation {
    item: String,
    quantity: u32,
}

impl RawItemMutation {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        field: &str,
        give: bool,
    ) -> Result<DialogueAction, ContentError> {
        validate_reference(path, npc, &format!("{field}.item"), &self.item, "item.")?;
        if self.quantity == 0 {
            return Err(issue(
                path,
                npc,
                format!("{field}.quantity"),
                "item quantity must be at least 1",
            ));
        }
        if give {
            Ok(DialogueAction::GiveItem {
                item_authored: self.item,
                quantity: self.quantity,
            })
        } else {
            Ok(DialogueAction::RemoveItem {
                item_authored: self.item,
                quantity: self.quantity,
            })
        }
    }
}

fn validate_reference(
    path: &Path,
    npc: &str,
    field: &str,
    value: &str,
    prefix: &str,
) -> Result<(), ContentError> {
    if !value.starts_with(prefix) {
        return Err(issue(
            path,
            npc,
            field,
            format!("must use the {prefix} authored-id namespace"),
        ));
    }
    validate_authored_id(value)
        .map_err(|error| issue(path, npc, field, format!("invalid authored id: {error:?}")))
}

fn require_npc(
    path: &Path,
    npc: &str,
    field: &str,
    target: &str,
    npc_beats: &HashMap<String, HashSet<String>>,
) -> Result<(), ContentError> {
    if npc_beats.contains_key(target) {
        Ok(())
    } else {
        Err(issue(
            path,
            npc,
            field,
            format!("references missing authored NPC '{target}'"),
        ))
    }
}

fn validate_fact(path: &Path, npc: &str, field: &str, fact: &str) -> Result<(), ContentError> {
    validate_authored_id(fact)
        .map_err(|error| issue(path, npc, field, format!("invalid fact id: {error:?}")))?;
    if fact.starts_with("npc.") && fact.ends_with(".met") {
        return Err(issue(
            path,
            npc,
            field,
            "NPC met state must use the typed npc_met condition/action",
        ));
    }
    Ok(())
}

fn validate_local_id(path: &Path, npc: &str, field: &str, value: &str) -> Result<(), ContentError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(issue(
            path,
            npc,
            field,
            "must be a non-empty lowercase [a-z0-9_] local id",
        ));
    }
    Ok(())
}

fn issue(
    path: &Path,
    definition: &str,
    field: impl Into<String>,
    reason: impl Into<String>,
) -> ContentError {
    ContentError::one(ValidationIssue::new(
        path.display().to_string(),
        definition,
        field,
        reason,
    ))
}

pub(crate) fn parse_raw(path: &Path, text: &str) -> Result<RawNpcDialogue, ContentError> {
    serde_json::from_str(text).map_err(|error| {
        ContentError::from_path(PathBuf::from(path), "-", "json", error.to_string())
    })
}
