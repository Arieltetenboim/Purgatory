import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import selection

REPO_ROOT = Path(__file__).resolve().parents[2]
WELCOME = REPO_ROOT / "content" / "authoring" / "npcs" / "welcome"


def load_npc(name):
    with (WELCOME / name).open("r", encoding="utf-8") as fh:
        return json.load(fh)


class NpcLabN6aPreviewTests(unittest.TestCase):
    def test_choice_marks_dialogue_heard_and_follows_continuation(self):
        doc = {
            "id": "npc.welcome.test",
            "interaction": {
                "beats": [
                    {
                        "id": "intro",
                        "title": "Intro",
                        "priority": 10,
                        "entry": True,
                        "pool": "once",
                        "conditions": [],
                        "lines": [{"text": "Hello."}],
                        "choices": [{"id": "ask", "text": "Ask.", "next": "detail"}],
                    },
                    {
                        "id": "detail",
                        "title": "Detail",
                        "priority": 0,
                        "entry": False,
                        "pool": "once",
                        "conditions": [],
                        "lines": [{"text": "Details."}],
                        "choices": [],
                    },
                ]
            },
        }
        state, next_beat = selection.advance_choice(doc, {}, "intro", "ask")
        self.assertEqual(next_beat["id"], "detail")
        self.assertIn(
            {"npc": "npc.welcome.test", "beat": "intro"}, state["dialogue_heard"]
        )

    def test_package_choice_applies_item_and_fact_actions(self):
        doc = load_npc("npc.welcome.traveler_stayed.json")
        state = {
            "facts": {
                "welcome.workshop.package_needed": True,
                "welcome.workshop.package_at_inn": True,
                "welcome.workshop.package_delivered": False,
            },
            "npc_met": ["npc.welcome.traveler_stayed"],
            "dialogue_heard": [],
            "item_owned": [],
            "item_equipped": [],
        }
        winner = selection.select_entry_beat(doc, state)
        self.assertEqual(winner["id"], "workshop_package_unknown")

        new_state, next_beat = selection.advance_choice(
            doc, state, winner["id"], "take_package"
        )
        self.assertIsNone(next_beat)
        self.assertIn("item.package", new_state["item_owned"])
        self.assertFalse(new_state["facts"]["welcome.workshop.package_at_inn"])
        self.assertIn(
            {"npc": "npc.welcome.traveler_stayed", "beat": winner["id"]},
            new_state["dialogue_heard"],
        )
        self.assertNotIn("item.package", state["item_owned"])
        self.assertTrue(state["facts"]["welcome.workshop.package_at_inn"])

    def test_first_meeting_choice_marks_npc_met(self):
        doc = load_npc("npc.welcome.traveler_stayed.json")
        state = {
            "facts": {},
            "npc_met": [],
            "dialogue_heard": [],
            "item_owned": [],
            "item_equipped": [],
        }
        winner = selection.select_entry_beat(doc, state)
        self.assertEqual(winner["id"], "intro")
        new_state, next_beat = selection.advance_choice(
            doc, state, "intro", "ask_place"
        )
        self.assertEqual(next_beat["id"], "intro_place")
        self.assertIn("npc.welcome.traveler_stayed", new_state["npc_met"])

    def test_complete_beat_records_once_memory_without_mutating_input(self):
        doc = load_npc("npc.welcome.traveler_stayed.json")
        state = {
            "facts": {},
            "npc_met": ["npc.welcome.traveler_stayed"],
            "dialogue_heard": [],
            "item_owned": [],
            "item_equipped": [],
        }
        new_state = selection.complete_beat(doc, state, "lore_roofs")
        self.assertIn(
            {"npc": "npc.welcome.traveler_stayed", "beat": "lore_roofs"},
            new_state["dialogue_heard"],
        )
        self.assertEqual(state["dialogue_heard"], [])
        self.assertEqual(selection.select_entry_beat(doc, new_state)["id"], "filler_food")


class NpcLabN6bDiagnosticsTests(unittest.TestCase):
    def test_diagnostics_report_same_winner_as_selector(self):
        doc = load_npc("npc.welcome.traveler_stayed.json")
        state = {
            "facts": {},
            "npc_met": [],
            "dialogue_heard": [],
            "item_owned": [],
            "item_equipped": [],
        }
        winner = selection.select_entry_beat(doc, state)
        report = selection.explain_entry_selection(doc, state)
        self.assertEqual(report["winner_id"], winner["id"])
        winner_rows = [row for row in report["beats"] if row["status"] == "winner"]
        self.assertEqual([row["id"] for row in winner_rows], [winner["id"]])

    def test_failed_condition_reports_expected_and_actual(self):
        doc = load_npc("npc.welcome.traveler_stayed.json")
        state = {
            "facts": {
                "welcome.workshop.package_needed": False,
                "welcome.workshop.package_at_inn": False,
                "welcome.workshop.package_delivered": False,
            },
            "npc_met": ["npc.welcome.traveler_stayed"],
            "dialogue_heard": [],
            "item_owned": [],
            "item_equipped": [],
        }
        report = selection.explain_entry_selection(doc, state)
        row = next(value for value in report["beats"] if value["id"] == "workshop_package_unknown")
        self.assertEqual(row["status"], "rejected")
        failures = [value for value in row["conditions"] if not value["matched"]]
        self.assertTrue(failures)
        self.assertTrue(any(value["expected"] is True and value["actual"] is False for value in failures))

    def test_once_pool_rejection_is_explained_after_heard(self):
        doc = load_npc("npc.welcome.traveler_stayed.json")
        state = {
            "facts": {},
            "npc_met": ["npc.welcome.traveler_stayed"],
            "dialogue_heard": [
                {"npc": "npc.welcome.traveler_stayed", "beat": "lore_roofs"}
            ],
            "item_owned": [],
            "item_equipped": [],
        }
        report = selection.explain_entry_selection(doc, state)
        lore = next(value for value in report["beats"] if value["id"] == "lore_roofs")
        filler = next(value for value in report["beats"] if value["id"] == "filler_food")
        self.assertEqual(lore["status"], "rejected")
        self.assertFalse(lore["pool_result"]["allowed"])
        self.assertIn("already", lore["pool_result"]["reason"].lower())
        self.assertEqual(filler["status"], "winner")

    def test_lower_priority_eligible_beat_is_not_reported_rejected(self):
        doc = {
            "id": "npc.welcome.test",
            "interaction": {
                "beats": [
                    {"id": "high", "title": "High", "priority": 10, "entry": True, "pool": "mandatory", "conditions": [], "lines": [], "choices": []},
                    {"id": "low", "title": "Low", "priority": 1, "entry": True, "pool": "repeatable", "conditions": [], "lines": [], "choices": []},
                ]
            },
        }
        report = selection.explain_entry_selection(doc, {})
        low = next(value for value in report["beats"] if value["id"] == "low")
        self.assertEqual(report["winner_id"], "high")
        self.assertEqual(low["status"], "eligible")
        self.assertTrue(low["eligible"])
        self.assertIn("below winner", low["reason"])

    def test_continuation_is_reported_outside_entry_selection(self):
        doc = {
            "id": "npc.welcome.test",
            "interaction": {
                "beats": [
                    {"id": "entry", "title": "Entry", "priority": 1, "entry": True, "pool": "mandatory", "conditions": [], "lines": [], "choices": []},
                    {"id": "detail", "title": "Detail", "priority": 100, "entry": False, "pool": "once", "conditions": [], "lines": [], "choices": []},
                ]
            },
        }
        report = selection.explain_entry_selection(doc, {})
        detail = next(value for value in report["beats"] if value["id"] == "detail")
        self.assertEqual(detail["status"], "continuation")
        self.assertFalse(detail["eligible"])


if __name__ == "__main__":
    unittest.main()
