import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import selection
import server

REPO_ROOT = Path(__file__).resolve().parents[2]
WELCOME_NPCS = REPO_ROOT / "content" / "authoring" / "npcs" / "welcome"


def valid_beat(beat_id="entry", *, entry=True):
    return {
        "id": beat_id,
        "title": beat_id,
        "priority": 10,
        "entry": entry,
        "conditions": [],
        "lines": [{"text": "Hello."}],
        "choices": [],
    }


def load_welcome_npc(filename):
    with (WELCOME_NPCS / filename).open("r", encoding="utf-8") as fh:
        return json.load(fh)


class NpcLabN4RepairTests(unittest.TestCase):
    def test_minimal_document_validates(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        self.assertEqual(server.validate_npc_document(doc), [])

    def test_fact_condition_validates(self):
        self.assertEqual(
            server.validate_condition(
                {"fact": "welcome.workshop.package_needed", "equals": True}
            ),
            [],
        )

    def test_typed_npc_met_condition_validates(self):
        self.assertEqual(
            server.validate_condition(
                {"npc_met": "npc.welcome.workshop_craftsperson", "equals": True}
            ),
            [],
        )

    def test_npc_met_fact_is_rejected(self):
        errors = server.validate_condition(
            {"fact": "npc.welcome.workshop_craftsperson.met", "equals": True}
        )
        self.assertTrue(any("npc_met" in error for error in errors))

    def test_dialogue_heard_condition_validates(self):
        self.assertEqual(
            server.validate_condition(
                {
                    "dialogue_heard": {
                        "npc": "npc.welcome.workshop_craftsperson",
                        "beat": "intro",
                    },
                    "equals": True,
                }
            ),
            [],
        )

    def test_item_owned_and_equipped_validate(self):
        self.assertEqual(
            server.validate_condition(
                {"item_owned": "item.workshop.package", "equals": True}
            ),
            [],
        )
        self.assertEqual(
            server.validate_condition(
                {"item_equipped": "item.workshop.boots", "equals": False}
            ),
            [],
        )

    def test_mark_npc_met_action_validates(self):
        self.assertEqual(
            server.validate_action(
                {"mark_npc_met": "npc.welcome.workshop_craftsperson"}
            ),
            [],
        )

    def test_set_fact_cannot_fake_npc_met(self):
        errors = server.validate_action(
            {
                "set_fact": {
                    "fact": "npc.welcome.workshop_craftsperson.met",
                    "value": True,
                }
            }
        )
        self.assertTrue(any("mark_npc_met" in error for error in errors))

    def test_regular_actions_validate(self):
        self.assertEqual(
            server.validate_action(
                {"set_fact": {"fact": "welcome.workshop.package_at_inn", "value": False}}
            ),
            [],
        )
        self.assertEqual(
            server.validate_action(
                {"give_item": {"item": "item.workshop.package", "quantity": 1}}
            ),
            [],
        )
        self.assertEqual(
            server.validate_action(
                {"remove_item": {"item": "item.workshop.package", "quantity": 1}}
            ),
            [],
        )

    def test_beat_requires_explicit_entry_role(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        beat = valid_beat()
        del beat["entry"]
        doc["interaction"]["beats"] = [beat]
        errors = server.validate_npc_document(doc)
        self.assertTrue(any(".entry" in error for error in errors))

    def test_continuation_beat_is_valid(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["interaction"]["beats"] = [valid_beat("followup", entry=False)]
        self.assertEqual(server.validate_npc_document(doc), [])

    def test_choice_can_transition_to_continuation(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        entry = valid_beat("intro", entry=True)
        entry["choices"] = [
            {"id": "ask", "text": "Ask.", "next": "detail", "actions": []}
        ]
        doc["interaction"]["beats"] = [entry, valid_beat("detail", entry=False)]
        self.assertEqual(server.validate_npc_document(doc), [])

    def test_broken_next_reference_is_rejected(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        entry = valid_beat("intro", entry=True)
        entry["choices"] = [
            {"id": "ask", "text": "Ask.", "next": "missing", "actions": []}
        ]
        doc["interaction"]["beats"] = [entry]
        errors = server.validate_npc_document(doc)
        self.assertTrue(any("missing beat" in error for error in errors))

    def test_duplicate_beat_id_is_rejected(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["interaction"]["beats"] = [
            valid_beat("same", entry=True),
            valid_beat("same", entry=False),
        ]
        errors = server.validate_npc_document(doc)
        self.assertTrue(any("duplicates beat id" in error for error in errors))

    def test_combined_choice_actions_validate(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        beat = valid_beat("offer", entry=True)
        beat["choices"] = [
            {
                "id": "accept",
                "text": "All right.",
                "next": None,
                "actions": [
                    {"give_item": {"item": "item.workshop.package", "quantity": 1}},
                    {"mark_npc_met": "npc.welcome.traveler_stayed"},
                    {
                        "set_fact": {
                            "fact": "welcome.workshop.package_at_inn",
                            "value": False,
                        }
                    },
                ],
            }
        ]
        doc["interaction"]["beats"] = [beat]
        self.assertEqual(server.validate_npc_document(doc), [])

    def test_hebrew_authored_content_is_rejected(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["design"]["working_name"] = "Test " + chr(0x05D0)
        errors = server.validate_npc_document(doc)
        self.assertTrue(any("English-only" in error for error in errors))

    def test_path_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                server.resolve_npc_path(Path(tmp), "../outside.json")

    def test_normal_nested_path_is_allowed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            resolved = server.resolve_npc_path(root, "welcome/npc.welcome.test.json")
            self.assertEqual(
                resolved,
                (root / "welcome" / "npc.welcome.test.json").resolve(),
            )


class NpcLabN4SelectionTests(unittest.TestCase):
    def test_all_conditions_are_and_and_continuations_are_excluded(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        fallback = valid_beat("fallback", entry=True)
        fallback["priority"] = 1
        specific = valid_beat("specific", entry=True)
        specific["priority"] = 10
        specific["conditions"] = [
            {"fact": "a", "equals": True},
            {"fact": "b", "equals": True},
        ]
        continuation = valid_beat("continuation", entry=False)
        continuation["priority"] = 100
        doc["interaction"]["beats"] = [fallback, specific, continuation]

        winner = selection.select_entry_beat(doc, {"facts": {"a": True, "b": False}})
        self.assertEqual(winner["id"], "fallback")

        winner = selection.select_entry_beat(doc, {"facts": {"a": True, "b": True}})
        self.assertEqual(winner["id"], "specific")

    def test_equal_priority_preserves_authored_order(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        first = valid_beat("first")
        second = valid_beat("second")
        doc["interaction"]["beats"] = [first, second]
        self.assertEqual(selection.select_entry_beat(doc, {})["id"], "first")

    def test_typed_condition_values(self):
        state = {
            "facts": {"fact.a": True},
            "npc_met": ["npc.welcome.a"],
            "dialogue_heard": [{"npc": "npc.welcome.a", "beat": "intro"}],
            "item_owned": ["item.a"],
            "item_equipped": ["item.b"],
        }
        conditions = [
            {"fact": "fact.a", "equals": True},
            {"npc_met": "npc.welcome.a", "equals": True},
            {"dialogue_heard": {"npc": "npc.welcome.a", "beat": "intro"}, "equals": True},
            {"item_owned": "item.a", "equals": True},
            {"item_equipped": "item.b", "equals": True},
            {"item_owned": "item.missing", "equals": False},
        ]
        self.assertTrue(all(selection.condition_matches(c, state) for c in conditions))

    def test_traveler_package_dialogue_changes_after_craftsperson_met(self):
        doc = load_welcome_npc("npc.welcome.traveler_stayed.json")
        base_state = {
            "facts": {
                "welcome.workshop.package_needed": True,
                "welcome.workshop.package_at_inn": True,
                "welcome.workshop.package_delivered": False,
            },
            "npc_met": ["npc.welcome.traveler_stayed"],
            "item_owned": [],
        }

        winner = selection.select_entry_beat(doc, base_state)
        self.assertEqual(winner["id"], "workshop_package_unknown")

        workshop_first = dict(base_state)
        workshop_first["npc_met"] = [
            "npc.welcome.traveler_stayed",
            "npc.welcome.workshop_craftsperson",
        ]
        winner = selection.select_entry_beat(doc, workshop_first)
        self.assertEqual(winner["id"], "workshop_package_known")

    def test_workshop_first_meeting_changes_with_package_ownership(self):
        doc = load_welcome_npc("npc.welcome.workshop_craftsperson.json")
        waiting_state = {
            "facts": {
                "welcome.workshop.package_needed": True,
                "welcome.workshop.package_at_inn": True,
                "welcome.workshop.package_delivered": False,
            },
            "npc_met": [],
            "item_owned": [],
        }
        winner = selection.select_entry_beat(doc, waiting_state)
        self.assertEqual(winner["id"], "package_waiting_first_meeting")

        carrying_state = {
            "facts": {
                "welcome.workshop.package_needed": True,
                "welcome.workshop.package_at_inn": False,
                "welcome.workshop.package_delivered": False,
            },
            "npc_met": [],
            "item_owned": ["item.package"],
        }
        winner = selection.select_entry_beat(doc, carrying_state)
        self.assertEqual(winner["id"], "package_delivery_first_meeting")


if __name__ == "__main__":
    unittest.main()
