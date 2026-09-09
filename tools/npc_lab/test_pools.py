import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import selection

REPO_ROOT = Path(__file__).resolve().parents[2]
TRAVELER_PATH = (
    REPO_ROOT
    / "content"
    / "authoring"
    / "npcs"
    / "welcome"
    / "npc.welcome.traveler_stayed.json"
)


def beat(beat_id, pool, priority=10):
    return {
        "id": beat_id,
        "title": beat_id,
        "priority": priority,
        "entry": True,
        "pool": pool,
        "conditions": [],
        "lines": [{"text": "Hello."}],
        "choices": [],
    }


def document(*beats):
    return {
        "schema_version": 1,
        "id": "npc.welcome.test",
        "interaction": {"beats": list(beats)},
    }


class NpcLabN5aPoolSelectionTests(unittest.TestCase):
    def test_once_is_excluded_after_it_was_heard(self):
        doc = document(
            beat("once_line", selection.POOL_ONCE, 10),
            beat("fallback", selection.POOL_REPEATABLE, 1),
        )
        self.assertEqual(selection.select_entry_beat(doc, {})["id"], "once_line")

        heard = {
            "dialogue_heard": [
                {"npc": "npc.welcome.test", "beat": "once_line"}
            ]
        }
        self.assertEqual(selection.select_entry_beat(doc, heard)["id"], "fallback")

    def test_repeatable_remains_eligible_after_it_was_heard(self):
        doc = document(beat("filler", selection.POOL_REPEATABLE, 1))
        state = {
            "dialogue_heard": [
                {"npc": "npc.welcome.test", "beat": "filler"}
            ]
        }
        self.assertEqual(selection.select_entry_beat(doc, state)["id"], "filler")

    def test_lore_is_optional_one_shot_then_repeatable_fallback_wins(self):
        doc = document(
            beat("lore", selection.POOL_LORE, 5),
            beat("filler", selection.POOL_REPEATABLE, 1),
        )
        self.assertEqual(selection.select_entry_beat(doc, {})["id"], "lore")

        state = {
            "dialogue_heard": [
                {"npc": "npc.welcome.test", "beat": "lore"}
            ]
        }
        self.assertEqual(selection.select_entry_beat(doc, state)["id"], "filler")

    def test_mandatory_is_not_suppressed_by_dialogue_memory(self):
        doc = document(
            beat("important", selection.POOL_MANDATORY, 50),
            beat("filler", selection.POOL_REPEATABLE, 1),
        )
        state = {
            "dialogue_heard": [
                {"npc": "npc.welcome.test", "beat": "important"}
            ]
        }
        self.assertEqual(selection.select_entry_beat(doc, state)["id"], "important")

    def test_rare_is_not_auto_selected_until_cadence_policy_exists(self):
        doc = document(
            beat("rare", selection.POOL_RARE, 100),
            beat("filler", selection.POOL_REPEATABLE, 1),
        )
        self.assertEqual(selection.select_entry_beat(doc, {})["id"], "filler")

    def test_unknown_explicit_pool_fails_loudly(self):
        doc = document(beat("bad", "sometimes", 10))
        with self.assertRaises(ValueError):
            selection.select_entry_beat(doc, {})

    def test_real_traveler_moves_from_lore_to_repeatable_without_progression_change(self):
        with TRAVELER_PATH.open("r", encoding="utf-8") as fh:
            doc = json.load(fh)

        base_state = {
            "facts": {},
            "npc_met": ["npc.welcome.traveler_stayed"],
            "dialogue_heard": [],
            "item_owned": [],
            "item_equipped": [],
        }
        winner = selection.select_entry_beat(doc, base_state)
        self.assertEqual(winner["id"], "lore_roofs")

        after_lore = dict(base_state)
        after_lore["dialogue_heard"] = [
            {"npc": "npc.welcome.traveler_stayed", "beat": "lore_roofs"}
        ]
        winner = selection.select_entry_beat(doc, after_lore)
        self.assertEqual(winner["id"], "filler_food")
        self.assertEqual(after_lore["facts"], base_state["facts"])


if __name__ == "__main__":
    unittest.main()
