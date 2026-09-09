"""Pure dialogue selection for NPC Lab authoring/tests.

N4 owns typed state conditions and deterministic ENTRY selection.
N5a adds only the pool semantics that are already well-defined by authored
Welcome content. Runtime projection remains deferred to N10.

The caller is expected to pass an NPC document that already passed server.py
validation.
"""

from __future__ import annotations

from typing import Any

POOL_MANDATORY = "mandatory"
POOL_ONCE = "once"
POOL_REPEATABLE = "repeatable"
POOL_RARE = "rare"
POOL_LORE = "lore"
SUPPORTED_POOLS = {
    POOL_MANDATORY,
    POOL_ONCE,
    POOL_REPEATABLE,
    POOL_RARE,
    POOL_LORE,
}


def _contains_dialogue_heard(state: dict[str, Any], npc: str, beat: str) -> bool:
    for value in state.get("dialogue_heard", []):
        if not isinstance(value, dict):
            continue
        if value.get("npc") == npc and value.get("beat") == beat:
            return True
    return False


def condition_value(condition: dict[str, Any], state: dict[str, Any]) -> bool:
    """Return the current boolean value for one validated N4 condition."""
    if "fact" in condition:
        facts = state.get("facts", {})
        return bool(facts.get(condition["fact"], False)) if isinstance(facts, dict) else False

    if "npc_met" in condition:
        return condition["npc_met"] in state.get("npc_met", [])

    if "dialogue_heard" in condition:
        reference = condition["dialogue_heard"]
        return _contains_dialogue_heard(state, reference["npc"], reference["beat"])

    if "item_owned" in condition:
        return condition["item_owned"] in state.get("item_owned", [])

    if "item_equipped" in condition:
        return condition["item_equipped"] in state.get("item_equipped", [])

    raise ValueError("Unsupported N4 condition shape.")


def condition_matches(condition: dict[str, Any], state: dict[str, Any]) -> bool:
    expected = condition.get("equals")
    if not isinstance(expected, bool):
        raise ValueError("Validated N4 condition must contain boolean equals.")
    return condition_value(condition, state) == expected


def pool_allows_selection(
    document: dict[str, Any], beat: dict[str, Any], state: dict[str, Any]
) -> bool:
    """Apply the currently frozen N5a pool semantics.

    - mandatory: normal condition/priority selection;
    - once: eligible until this NPC beat has been heard;
    - repeatable: remains eligible after being heard;
    - lore: optional one-shot content, eligible until heard;
    - rare: deliberately not auto-selected yet because no cadence/chance policy
      has been authored. N5b+ must define that policy before rare affects selection.

    A missing pool keeps legacy N4 tests/content behavior and is treated as
    mandatory. Unknown explicit pool labels fail loudly rather than silently
    inventing behavior.
    """
    pool = beat.get("pool")
    if pool is None:
        pool = POOL_MANDATORY
    if pool not in SUPPORTED_POOLS:
        raise ValueError(f"Unsupported dialogue pool: {pool!r}.")

    if pool == POOL_RARE:
        return False
    if pool in (POOL_ONCE, POOL_LORE):
        npc = document.get("id")
        beat_id = beat.get("id")
        if not isinstance(npc, str) or not isinstance(beat_id, str):
            raise ValueError("Pool memory selection requires validated NPC and beat ids.")
        return not _contains_dialogue_heard(state, npc, beat_id)
    return True


def beat_matches(
    document: dict[str, Any], beat: dict[str, Any], state: dict[str, Any]
) -> bool:
    """Return whether a validated ENTRY beat is eligible for top-level selection."""
    if beat.get("entry") is not True:
        return False
    if not all(condition_matches(condition, state) for condition in beat.get("conditions", [])):
        return False
    return pool_allows_selection(document, beat, state)


def select_entry_beat(document: dict[str, Any], state: dict[str, Any]) -> dict[str, Any] | None:
    """Select the highest-priority eligible ENTRY beat.

    Equal priorities preserve authored JSON order. Pool semantics may exclude a
    beat before priority comparison, but they do not introduce random selection.
    """
    interaction = document.get("interaction", {})
    beats = interaction.get("beats", []) if isinstance(interaction, dict) else []

    winner: dict[str, Any] | None = None
    for beat in beats:
        if not isinstance(beat, dict) or not beat_matches(document, beat, state):
            continue
        if winner is None or beat["priority"] > winner["priority"]:
            winner = beat
    return winner
