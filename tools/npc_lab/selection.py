"""Pure dialogue selection and synthetic progression for NPC Lab tests.

N4 owns typed state conditions and deterministic ENTRY selection.
N5 adds the currently frozen dialogue-pool semantics.
N6a reuses those rules for a local synthetic conversation preview and adds only
synthetic choice/action progression. N6b exposes structured diagnostics from the
same evaluator. Runtime projection remains deferred to N10.

The caller is expected to pass an NPC document that already passed server.py
validation.
"""

from __future__ import annotations

import copy
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


def _append_unique(state: dict[str, Any], key: str, value: str) -> None:
    items = state.setdefault(key, [])
    if not isinstance(items, list):
        raise ValueError(f"Synthetic state field {key!r} must be a list.")
    if value not in items:
        items.append(value)


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
    """Apply the currently frozen N5 pool semantics."""
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


def _condition_reference(condition: dict[str, Any]) -> dict[str, Any]:
    for key in ("fact", "npc_met", "dialogue_heard", "item_owned", "item_equipped"):
        if key in condition:
            return {"kind": key, "reference": condition[key]}
    raise ValueError("Unsupported N4 condition shape.")


def _pool_diagnostic(
    document: dict[str, Any], beat: dict[str, Any], state: dict[str, Any]
) -> dict[str, Any]:
    pool = beat.get("pool") or POOL_MANDATORY
    allowed = pool_allows_selection(document, beat, state)
    if pool == POOL_RARE:
        reason = "Rare automatic cadence is not defined, so this beat is not auto-selected."
    elif pool in (POOL_ONCE, POOL_LORE) and not allowed:
        reason = "This one-shot beat is already present in Dialogue Heard."
    elif pool in (POOL_ONCE, POOL_LORE):
        reason = "This one-shot beat has not been heard yet."
    elif pool == POOL_REPEATABLE:
        reason = "Repeatable pool remains eligible regardless of Dialogue Heard."
    else:
        reason = "Mandatory pool does not suppress an otherwise eligible beat."
    return {"pool": pool, "allowed": allowed, "reason": reason}


def explain_entry_selection(
    document: dict[str, Any], state: dict[str, Any]
) -> dict[str, Any]:
    """Return structured N6b diagnostics without changing selection semantics."""
    winner = select_entry_beat(document, state)
    winner_id = winner.get("id") if winner is not None else None
    winner_priority = winner.get("priority") if winner is not None else None

    interaction = document.get("interaction", {})
    beats = interaction.get("beats", []) if isinstance(interaction, dict) else []
    diagnostics: list[dict[str, Any]] = []

    for authored_index, beat in enumerate(beats):
        if not isinstance(beat, dict):
            continue

        is_entry = beat.get("entry") is True
        condition_results: list[dict[str, Any]] = []
        for condition in beat.get("conditions", []):
            expected = condition["equals"]
            actual = condition_value(condition, state)
            reference = _condition_reference(condition)
            condition_results.append(
                {
                    **reference,
                    "expected": expected,
                    "actual": actual,
                    "matched": actual == expected,
                }
            )

        conditions_match = all(item["matched"] for item in condition_results)
        pool_info = _pool_diagnostic(document, beat, state) if is_entry else None
        eligible = is_entry and conditions_match and bool(pool_info and pool_info["allowed"])

        if not is_entry:
            status = "continuation"
            reason = "CONTINUATION beats do not participate in top-level ENTRY selection."
        elif not conditions_match:
            failed = sum(1 for item in condition_results if not item["matched"])
            status = "rejected"
            reason = f"Rejected: {failed} condition(s) failed."
        elif pool_info is not None and not pool_info["allowed"]:
            status = "rejected"
            reason = f"Rejected by pool: {pool_info['reason']}"
        elif beat.get("id") == winner_id:
            status = "winner"
            reason = "Selected: highest-priority eligible ENTRY beat."
        elif winner_priority is not None and beat["priority"] < winner_priority:
            status = "eligible"
            reason = f"Eligible, but P{beat['priority']} is below winner P{winner_priority}."
        else:
            status = "eligible"
            reason = "Eligible at the winning priority, but appears later in authored order."

        diagnostics.append(
            {
                "id": beat.get("id"),
                "title": beat.get("title"),
                "authored_index": authored_index,
                "entry": is_entry,
                "priority": beat.get("priority"),
                "pool": beat.get("pool") or POOL_MANDATORY,
                "conditions": condition_results,
                "conditions_match": conditions_match,
                "pool_result": pool_info,
                "eligible": eligible,
                "status": status,
                "reason": reason,
            }
        )

    eligible_ids = [item["id"] for item in diagnostics if item["eligible"]]
    rejected_ids = [item["id"] for item in diagnostics if item["status"] == "rejected"]
    return {
        "winner_id": winner_id,
        "eligible_ids": eligible_ids,
        "rejected_ids": rejected_ids,
        "beats": diagnostics,
    }


def find_beat(document: dict[str, Any], beat_id: str) -> dict[str, Any]:
    interaction = document.get("interaction", {})
    beats = interaction.get("beats", []) if isinstance(interaction, dict) else []
    for beat in beats:
        if isinstance(beat, dict) and beat.get("id") == beat_id:
            return beat
    raise ValueError(f"Unknown beat id: {beat_id!r}.")


def _record_dialogue_heard(
    document: dict[str, Any], state: dict[str, Any], beat_id: str
) -> None:
    npc = document.get("id")
    if not isinstance(npc, str):
        raise ValueError("Synthetic dialogue memory requires a validated NPC id.")
    heard = state.setdefault("dialogue_heard", [])
    if not isinstance(heard, list):
        raise ValueError("Synthetic state field 'dialogue_heard' must be a list.")
    if not _contains_dialogue_heard(state, npc, beat_id):
        heard.append({"npc": npc, "beat": beat_id})


def _apply_actions(state: dict[str, Any], actions: list[Any]) -> None:
    """Apply the closed N4 action vocabulary to N6a synthetic state.

    Synthetic item state is boolean ownership/equipment only. Quantity 1 is the
    only supported item mutation until authored content demonstrates a need for
    stack/count simulation.
    """
    for action in actions:
        if not isinstance(action, dict):
            raise ValueError("Synthetic action must be an object.")

        if "set_fact" in action:
            data = action["set_fact"]
            facts = state.setdefault("facts", {})
            if not isinstance(facts, dict):
                raise ValueError("Synthetic state field 'facts' must be an object.")
            facts[data["fact"]] = data["value"]
            continue

        if "mark_npc_met" in action:
            _append_unique(state, "npc_met", action["mark_npc_met"])
            continue

        for kind, state_key in (("give_item", "item_owned"), ("remove_item", "item_owned")):
            if kind not in action:
                continue
            data = action[kind]
            if data["quantity"] != 1:
                raise ValueError(
                    "N6a synthetic item actions currently support quantity 1 only."
                )
            item = data["item"]
            if kind == "give_item":
                _append_unique(state, state_key, item)
            else:
                items = state.setdefault(state_key, [])
                if not isinstance(items, list):
                    raise ValueError(f"Synthetic state field {state_key!r} must be a list.")
                while item in items:
                    items.remove(item)
            break
        else:
            raise ValueError("Unsupported synthetic action shape.")


def complete_beat(
    document: dict[str, Any], state: dict[str, Any], beat_id: str
) -> dict[str, Any]:
    """Return cloned synthetic state with one completed beat recorded as heard."""
    find_beat(document, beat_id)
    next_state = copy.deepcopy(state)
    _record_dialogue_heard(document, next_state, beat_id)
    return next_state


def advance_choice(
    document: dict[str, Any], state: dict[str, Any], beat_id: str, choice_id: str
) -> tuple[dict[str, Any], dict[str, Any] | None]:
    """Complete a beat, apply its chosen actions, and follow explicit Next Beat.

    N6a deliberately does not invent extra condition gating for explicit `next`
    transitions. ENTRY conditions/pools remain owned by select_entry_beat().
    """
    beat = find_beat(document, beat_id)
    choices = beat.get("choices", [])
    if not isinstance(choices, list):
        raise ValueError("Validated beat choices must be a list.")

    choice = next(
        (
            value
            for value in choices
            if isinstance(value, dict) and value.get("id") == choice_id
        ),
        None,
    )
    if choice is None:
        raise ValueError(f"Unknown choice id {choice_id!r} on beat {beat_id!r}.")

    next_state = copy.deepcopy(state)
    _record_dialogue_heard(document, next_state, beat_id)
    actions = choice.get("actions", [])
    if not isinstance(actions, list):
        raise ValueError("Validated choice actions must be a list.")
    _apply_actions(next_state, actions)

    next_id = choice.get("next")
    if next_id is None:
        return next_state, None
    if not isinstance(next_id, str):
        raise ValueError("Validated choice next must be a beat id or null.")
    return next_state, find_beat(document, next_id)
