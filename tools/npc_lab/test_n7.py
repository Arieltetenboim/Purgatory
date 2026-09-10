import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import server
import server_n6


class NpcLabN7ValidationTests(unittest.TestCase):
    def test_supported_pool_is_accepted(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["interaction"]["beats"] = [
            {
                "id": "intro",
                "title": "Intro",
                "priority": 1,
                "entry": True,
                "pool": "once",
                "conditions": [],
                "lines": [{"text": "Hello."}],
                "choices": [],
            }
        ]
        self.assertEqual(server_n6.validate_npc_document_n7(doc), [])

    def test_invalid_pool_is_rejected_server_side(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["interaction"]["beats"] = [
            {
                "id": "intro",
                "title": "Intro",
                "priority": 1,
                "entry": True,
                "pool": "sometimes",
                "conditions": [],
                "lines": [{"text": "Hello."}],
                "choices": [],
            }
        ]
        errors = server_n6.validate_npc_document_n7(doc)
        self.assertTrue(any("pool must be one of" in error for error in errors))

    def test_missing_pool_remains_backward_compatible(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["interaction"]["beats"] = [
            {
                "id": "intro",
                "title": "Intro",
                "priority": 1,
                "entry": True,
                "conditions": [],
                "lines": [{"text": "Hello."}],
                "choices": [],
            }
        ]
        self.assertEqual(server_n6.validate_npc_document_n7(doc), [])

    def test_condition_requires_exactly_one_supported_check(self):
        errors = server.validate_condition(
            {"fact": "welcome.a", "npc_met": "npc.welcome.a", "equals": True}
        )
        self.assertTrue(any("exactly one" in error for error in errors))

    def test_condition_requires_explicit_boolean_equals(self):
        errors = server.validate_condition({"fact": "welcome.a"})
        self.assertTrue(any("equals" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
