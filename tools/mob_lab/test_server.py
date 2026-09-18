import tempfile
import unittest
from pathlib import Path

from server import (
    new_monster_document,
    resolve_monster_path,
    safe_filename,
    validate_monster_document,
)


class MobLabContractTests(unittest.TestCase):
    def test_default_document_matches_schema_v2_contract(self):
        doc = new_monster_document("monster.test.slime", "Test Slime")
        self.assertEqual([], validate_monster_document(doc))
        self.assertEqual("when_attacked", doc["behavior"]["aggro"])

    def test_invalid_runtime_values_are_rejected_before_save(self):
        doc = new_monster_document("monster.test.slime", "Test Slime")
        doc["health_max"] = 0
        doc["half_extents"] = [0.4, -0.1]
        doc["behavior"]["home_leash_radius"] = 0
        errors = validate_monster_document(doc)
        self.assertTrue(any("health_max" in item for item in errors))
        self.assertTrue(any("half_extents" in item for item in errors))
        self.assertTrue(any("home_leash_radius" in item for item in errors))

    def test_unknown_fields_are_rejected(self):
        doc = new_monster_document("monster.test.slime", "Test Slime")
        doc["acquisition_radius"] = 3.0
        errors = validate_monster_document(doc)
        self.assertTrue(any("unknown top-level" in item for item in errors))

    def test_path_cannot_escape_runtime_monster_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaises(ValueError):
                resolve_monster_path(root, "../outside.json")
            self.assertEqual(
                root / "monster.test.json",
                resolve_monster_path(root, "monster.test.json"),
            )

    def test_filename_uses_authored_id(self):
        self.assertEqual("monster.slime.blue.json", safe_filename("monster.slime.blue"))
        with self.assertRaises(ValueError):
            safe_filename("Monster Bad")


if __name__ == "__main__":
    unittest.main()
