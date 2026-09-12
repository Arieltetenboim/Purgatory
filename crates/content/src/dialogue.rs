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

    /// Select the highest-priority eligible ENTRY beat. Equal priorities keep
    /// authored order because replacement happens only for a strictly higher
    /// priority.
    #[must_use]
    pub fn select_entry(&self, state: &impl DialogueConditionState) -> Option<DialogueBeatIndex> {
        let mut winner: Option<(DialogueBeatIndex, i32)> = None;
        for (index, beat) in self.beats.iter().enumerate() {
            if beat.selection_role != DialogueSelectionRole::Entry
                || !beat
                    .conditions
                    .iter()
                    .all(|condition| condition.matches(state))
                || !beat
                    .pool
                    .allows_selection(state, &self.authored_id, &beat.id)
            {
                continue;
            }
            if winner.is_none_or(|(_, priority)| beat.priority > priority) {
                let index = u32::try_from(index).ok()?;
                winner = Some((DialogueBeatIndex::from_raw(index), beat.priority));
            }
        }
        winner.map(|(index, _)| index)
    }
}

/// Read-only authoritative state used by ENTRY selection. N10c supplies the
/// currently available player state; later narrative storage can implement the
/// same contract without changing authored selection rules.
pub trait DialogueConditionState {
    fn fact(&self, fact: &str) -> bool;
    fn npc_met(&self, npc_authored: &str) -> bool;
    fn dialogue_heard(&self, npc_authored: &str, beat_id: &str) -> bool;
    fn item_owned(&self, item_authored: &str) -> bool;
    fn item_equipped(&self, item_authored: &str) -> bool;
}

/// Client-safe Beat projection. Text and optional presentation cues are
/// present; conditions, pools, continuation, and actions remain exclusively
/// in [`NpcDialogueDefinition`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpcDialoguePresentation {
    pub content_id: ContentId,
    pub authored_id: String,
    pub beats: Vec<DialoguePresentationBeat>,
}

impl NpcDialoguePresentation {
    #[must_use]
    pub fn beat(&self, beat: DialogueBeatIndex) -> Option<&DialoguePresentationBeat> {
        self.beats.get(beat.as_usize())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialoguePresentationBeat {
    pub lines: Vec<DialoguePresentationLine>,
    /// Client-safe choice labels only. Continuation and actions stay server-side.
    pub choices: Vec<DialoguePresentationChoice>,
    /// Precomposed once during content loading. A Beat is the visible and
    /// progressive unit; authored lines remain available for later cues.
    pub display_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialoguePresentationLine {
    pub text: String,
    /// Logical animation asset id resolved only by client presentation.
    pub animation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialoguePresentationChoice {
    pub text: String,
}

/// Compact resolved index into an NPC definition's authored beat order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl DialoguePool {
    fn allows_selection(
        self,
        state: &impl DialogueConditionState,
        npc_authored: &str,
        beat_id: &str,
    ) -> bool {
        match self {
            Self::Mandatory | Self::Repeatable => true,
            Self::Once | Self::Lore => !state.dialogue_heard(npc_authored, beat_id),
            // NPC Lab N5 deliberately has no automatic rare cadence yet.
            Self::Rare => false,
        }
    }
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

impl DialogueCondition {
    fn matches(&self, state: &impl DialogueConditionState) -> bool {
        let (actual, expected) = match self {
            Self::Fact { fact, equals } => (state.fact(fact), *equals),
            Self::NpcMet {
                npc_authored,
                equals,
            } => (state.npc_met(npc_authored), *equals),
            Self::DialogueHeard {
                npc_authored,
                beat_id,
                equals,
            } => (state.dialogue_heard(npc_authored, beat_id), *equals),
            Self::ItemOwned {
                item_authored,
                equals,
            } => (state.item_owned(item_authored), *equals),
            Self::ItemEquipped {
                item_authored,
                equals,
            } => (state.item_equipped(item_authored), *equals),
        };
        actual == expected
    }
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

    pub(crate) fn into_definitions(
        self,
        path: &Path,
        content_id: Option<ContentId>,
    ) -> Result<Option<(NpcDialogueDefinition, NpcDialoguePresentation)>, ContentError> {
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

        let resolved_beats = self
            .interaction
            .beats
            .into_iter()
            .enumerate()
            .map(|(index, beat)| beat.into_definition(path, &self.id, index, &beat_indices))
            .collect::<Result<Vec<_>, _>>()?;
        let (beats, presentation_beats) = resolved_beats.into_iter().unzip();

        let Some(content_id) = content_id else {
            return Ok(None);
        };
        let authored_id = self.id;
        Ok(Some((
            NpcDialogueDefinition {
                content_id,
                authored_id: authored_id.clone(),
                beats,
            },
            NpcDialoguePresentation {
                content_id,
                authored_id,
                beats: presentation_beats,
            },
        )))
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
    ) -> Result<(DialogueBeat, DialoguePresentationBeat), ContentError> {
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
        let resolved_lines = self
            .lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| line.into_definition(path, npc, beat_index, index))
            .collect::<Result<Vec<_>, _>>()?;
        let (lines, presentation_lines): (Vec<_>, Vec<_>) = resolved_lines.into_iter().unzip();

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

        let presentation_choices = choices
            .iter()
            .map(|choice| DialoguePresentationChoice {
                text: choice.text.clone(),
            })
            .collect();
        let display_text = presentation_lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok((
            DialogueBeat {
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
            },
            DialoguePresentationBeat {
                lines: presentation_lines,
                choices: presentation_choices,
                display_text,
            },
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    text: String,
    #[serde(default, rename = "voice")]
    _voice: Option<IgnoredAny>,
    #[serde(default)]
    animation: Option<String>,
}

impl RawLine {
    fn into_definition(
        self,
        path: &Path,
        npc: &str,
        beat_index: usize,
        line_index: usize,
    ) -> Result<(DialogueLine, DialoguePresentationLine), ContentError> {
        if self.text.trim().is_empty() {
            return Err(issue(
                path,
                npc,
                format!("interaction.beats[{beat_index}].lines[{line_index}].text"),
                "dialogue line text must not be empty",
            ));
        }
        if self
            .animation
            .as_ref()
            .is_some_and(|animation| animation.trim().is_empty())
        {
            return Err(issue(
                path,
                npc,
                format!("interaction.beats[{beat_index}].lines[{line_index}].animation"),
                "animation id must not be empty",
            ));
        }
        let presentation = DialoguePresentationLine {
            text: self.text.clone(),
            animation: self.animation,
        };
        Ok((DialogueLine { text: self.text }, presentation))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(Default)]
    struct State {
        facts: HashSet<String>,
        met: HashSet<String>,
        heard: HashSet<(String, String)>,
        owned: HashSet<String>,
        equipped: HashSet<String>,
    }

    impl DialogueConditionState for State {
        fn fact(&self, fact: &str) -> bool {
            self.facts.contains(fact)
        }

        fn npc_met(&self, npc_authored: &str) -> bool {
            self.met.contains(npc_authored)
        }

        fn dialogue_heard(&self, npc_authored: &str, beat_id: &str) -> bool {
            self.heard
                .contains(&(npc_authored.to_string(), beat_id.to_string()))
        }

        fn item_owned(&self, item_authored: &str) -> bool {
            self.owned.contains(item_authored)
        }

        fn item_equipped(&self, item_authored: &str) -> bool {
            self.equipped.contains(item_authored)
        }
    }

    fn beat(
        id: &str,
        role: DialogueSelectionRole,
        priority: i32,
        pool: DialoguePool,
        conditions: Vec<DialogueCondition>,
    ) -> DialogueBeat {
        DialogueBeat {
            id: id.into(),
            selection_role: role,
            priority,
            pool,
            conditions,
            lines: vec![DialogueLine { text: id.into() }],
            choices: Vec::new(),
        }
    }

    fn definition(beats: Vec<DialogueBeat>) -> NpcDialogueDefinition {
        NpcDialogueDefinition {
            content_id: ContentId::from_raw(20_001),
            authored_id: "npc.welcome.test".into(),
            beats,
        }
    }

    #[test]
    fn entry_selection_uses_priority_then_authored_order() {
        let definition = definition(vec![
            beat(
                "continuation",
                DialogueSelectionRole::Continuation,
                999,
                DialoguePool::Mandatory,
                Vec::new(),
            ),
            beat(
                "first",
                DialogueSelectionRole::Entry,
                10,
                DialoguePool::Mandatory,
                Vec::new(),
            ),
            beat(
                "second",
                DialogueSelectionRole::Entry,
                10,
                DialoguePool::Mandatory,
                Vec::new(),
            ),
        ]);
        assert_eq!(definition.select_entry(&State::default()).unwrap().raw(), 1);
    }

    #[test]
    fn entry_selection_matches_typed_conditions_and_pool_rules() {
        let definition = definition(vec![
            beat(
                "fallback",
                DialogueSelectionRole::Entry,
                1,
                DialoguePool::Repeatable,
                Vec::new(),
            ),
            beat(
                "once",
                DialogueSelectionRole::Entry,
                20,
                DialoguePool::Once,
                vec![
                    DialogueCondition::Fact {
                        fact: "welcome.ready".into(),
                        equals: true,
                    },
                    DialogueCondition::NpcMet {
                        npc_authored: "npc.welcome.other".into(),
                        equals: true,
                    },
                    DialogueCondition::DialogueHeard {
                        npc_authored: "npc.welcome.other".into(),
                        beat_id: "intro".into(),
                        equals: true,
                    },
                    DialogueCondition::ItemOwned {
                        item_authored: "item.package".into(),
                        equals: true,
                    },
                    DialogueCondition::ItemEquipped {
                        item_authored: "item.boots".into(),
                        equals: true,
                    },
                ],
            ),
            beat(
                "rare",
                DialogueSelectionRole::Entry,
                100,
                DialoguePool::Rare,
                Vec::new(),
            ),
        ]);
        let mut state = State::default();
        state.facts.insert("welcome.ready".into());
        state.met.insert("npc.welcome.other".into());
        state
            .heard
            .insert(("npc.welcome.other".into(), "intro".into()));
        state.owned.insert("item.package".into());
        state.equipped.insert("item.boots".into());
        assert_eq!(definition.select_entry(&state).unwrap().raw(), 1);

        state
            .heard
            .insert(("npc.welcome.test".into(), "once".into()));
        assert_eq!(definition.select_entry(&state).unwrap().raw(), 0);
    }

    #[test]
    fn client_projection_preserves_cues_without_adding_them_to_gameplay_lines() {
        let path = Path::new("npc.welcome.test.json");
        let raw = parse_raw(
            path,
            r#"{
                "schema_version": 1,
                "id": "npc.welcome.test",
                "interaction": { "beats": [{
                    "id": "intro",
                    "priority": 1,
                    "entry": true,
                    "conditions": [],
                    "lines": [
                        { "text": "hello", "animation": "dialogue_talk" },
                        { "text": "again", "animation": null }
                    ],
                    "choices": []
                }] }
            }"#,
        )
        .expect("raw dialogue");
        let (definition, presentation) = raw
            .into_definitions(path, Some(ContentId::from_raw(20_001)))
            .expect("valid projection")
            .expect("allocated NPC");
        let beat = presentation
            .beat(DialogueBeatIndex::from_raw(0))
            .expect("projected beat");
        assert_eq!(beat.display_text, "hello\n\nagain");
        assert_eq!(beat.lines[0].animation.as_deref(), Some("dialogue_talk"));
        assert_eq!(beat.lines[1].animation, None);
        assert_eq!(definition.beats[0].lines[0].text, "hello");
    }
}
