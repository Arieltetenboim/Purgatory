"""Pure N4 dialogue selection for NPC Lab authoring/tests.

This is authoring-tool logic only. Runtime projection remains deferred to N10.
The caller is expected to pass an NPC document that already passed server.py
validation.
"""

from __future__ import annotations

from typing import Any


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


def beat_matches(beat: dict[str, Any], state: dict[str, Any]) -> bool:
    """Return whether a validated ENTRY beat is eligible for top-level selection."""
    if beat.get("entry") is not True:
        return False
    return all(condition_matches(condition, state) for condition in beat.get("conditions", []))


def select_entry_beat(document: dict[str, Any], state: dict[str, Any]) -> dict[str, Any] | None:
    """Select the highest-priority eligible ENTRY beat.

    Equal priorities preserve authored JSON order, making selection deterministic
    without introducing N5 pool/random-selection semantics.
    """
    interaction = document.get("interaction", {})
    beats = interaction.get("beats", []) if isinstance(interaction, dict) else []

    winner: dict[str, Any] | None = None
    for beat in beats:
        if not isinstance(beat, dict) or not beat_matches(beat, state):
            continue
        if winner is None or beat["priority"] > winner["priority"]:
            winner = beat
    return winner
