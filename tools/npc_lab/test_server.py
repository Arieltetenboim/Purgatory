import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import server


class NpcLabN2Tests(unittest.TestCase):
    def test_minimal_document_validates(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        self.assertEqual(server.validate_npc_document(doc), [])

    def test_missing_id_is_rejected(self):
        errors = server.validate_npc_document({"schema_version": 1})
        self.assertTrue(any("id" in error for error in errors))

    def test_non_npc_namespace_is_rejected(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["id"] = "item.not_an_npc"
        errors = server.validate_npc_document(doc)
        self.assertTrue(any("npc.*" in error for error in errors))

    def test_hebrew_authored_content_is_rejected(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["design"]["working_name"] = "Test " + chr(0x05D0)
        errors = server.validate_npc_document(doc)
        self.assertTrue(any("English-only" in error for error in errors))

    def test_english_authored_content_is_allowed(self):
        doc = server.new_npc_document("npc.welcome.test", "welcome")
        doc["design"]["working_name"] = "The Test Traveler"
        doc["design"]["background"] = "A short English-only authoring proof."
        self.assertEqual(server.validate_npc_document(doc), [])

    def test_path_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaises(ValueError):
                server.resolve_npc_path(root, "../outside.json")

    def test_normal_nested_path_is_allowed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            resolved = server.resolve_npc_path(
                root, "welcome/npc.welcome.test.json"
            )
            self.assertEqual(
                resolved,
                (root / "welcome" / "npc.welcome.test.json").resolve(),
            )


if __name__ == "__main__":
    unittest.main()
