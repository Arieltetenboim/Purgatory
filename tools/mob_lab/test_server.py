import json
import tempfile
import unittest
from pathlib import Path

from server import (
    load_numeric_catalog,
    load_sprite_record,
    monster_constant_name,
    monster_reserved_ids,
    next_monster_content_id,
    prepare_monster_allocation,
    new_monster_document,
    resolve_monster_path,
    safe_filename,
    scan_sprite_manifests,
    validate_monster_document,
)


class MobLabContractTests(unittest.TestCase):
    def test_default_document_matches_schema_v4_contract(self):
        doc = new_monster_document("monster.test.slime", "Test Slime")
        self.assertEqual([], validate_monster_document(doc))
        self.assertEqual(4, doc["schema_version"])
        self.assertEqual("creature.red_slime", doc["sprite"])
        self.assertEqual(
            {"left": 0.4, "right": 0.4, "bottom": 0.6, "top": 0.6},
            doc["collision_bounds"],
        )
        self.assertEqual("when_attacked", doc["behavior"]["aggro"])

    def test_invalid_runtime_values_are_rejected_before_save(self):
        doc = new_monster_document("monster.test.slime", "Test Slime")
        doc["health_max"] = 0
        doc["sprite"] = ""
        doc["collision_bounds"]["bottom"] = -0.1
        doc["collision_bounds"]["top"] = 0.0
        doc["behavior"]["home_leash_radius"] = 0
        errors = validate_monster_document(doc)
        self.assertTrue(any("health_max" in item for item in errors))
        self.assertTrue(any("sprite" in item for item in errors))
        self.assertTrue(any("collision_bounds.bottom" in item for item in errors))
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

    def test_numeric_catalog_parser_reads_checked_monster_allocation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            catalog = root / "crates" / "common" / "src"
            catalog.mkdir(parents=True)
            (catalog / "content_catalog.rs").write_text(
                'pub const MONSTER_RED_SLIME: ContentId = ContentId::from_raw(10_001);\n'
                '        "monster.slime.red" => MONSTER_RED_SLIME,\n',
                encoding="utf-8",
            )
            self.assertEqual(
                {"monster.slime.red": 10001},
                load_numeric_catalog(root),
            )

    def test_sprite_scanner_discovers_manifest_without_hardcoded_mapping(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sprite_dir = root / "Graphic" / "creature" / "test"
            sprite_dir.mkdir(parents=True)
            (sprite_dir / "atlas.png").write_bytes(b"png")
            (sprite_dir / "manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "kind": "purgatory_sprite_animation",
                        "id": "creature.test",
                        "atlas": "atlas.png",
                        "frame_size_px": [64, 64],
                        "world_size": [1.0, 1.5],
                        "frame_seconds": 0.1,
                        "authored_facing": "right",
                        "clips": {
                            "move": {"frames": [0], "loop": True},
                            "idle": {"frames": [0], "loop": True},
                            "attack": {"frames": [0], "loop": False},
                        },
                    }
                ),
                encoding="utf-8",
            )
            items, issues = scan_sprite_manifests(root)
            self.assertEqual([], issues)
            self.assertEqual(["creature.test"], [item["id"] for item in items])
            self.assertEqual("creature.test", load_sprite_record(root, "creature.test")["id"])


    def test_monster_allocator_assigns_next_permanent_id_and_updates_catalog_and_ledger(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            common = root / "crates" / "common" / "src"
            common.mkdir(parents=True)
            (common / "content_catalog.rs").write_text(
                """use crate::ContentId;

pub const MONSTER_RED_SLIME: ContentId = ContentId::from_raw(10_001);
pub const MONSTER_MOSS_CRAB: ContentId = ContentId::from_raw(10_002);
pub const MONSTER_RETIRED_WISP: ContentId = ContentId::from_raw(10_004);

pub fn allocated_id_for_label(label: &str) -> Option<ContentId> {
    Some(match label {
        "monster.slime.red" => MONSTER_RED_SLIME,
        "monster.moss_crab" => MONSTER_MOSS_CRAB,
        _ => return None,
    })
}

pub fn label_for_allocated_id(id: ContentId) -> Option<&'static str> {
    Some(match id {
        MONSTER_RED_SLIME => "monster.slime.red",
        MONSTER_MOSS_CRAB => "monster.moss_crab",
        _ => return None,
    })
}
""",
                encoding="utf-8",
            )
            content = root / "content"
            content.mkdir(parents=True)
            (content / "CONTENT_ID_CATALOG.md").write_text(
                """# Content ID Catalog

### Monsters — 10,000–19,999

| ID | Label | Status |
| ---: | --- | --- |
| `10001` | `monster.slime.red` | active |
| `10002` | `monster.moss_crab` | active |

### NPCs — 20,000–29,999
""",
                encoding="utf-8",
            )

            self.assertEqual({10001, 10002, 10003}, monster_reserved_ids(root))
            self.assertEqual(10004, next_monster_content_id(root))
            self.assertEqual("MONSTER_CAVE_BAT", monster_constant_name("monster.cave_bat"))
            content_id, writes = prepare_monster_allocation(root, "monster.cave_bat")
            self.assertEqual(10004, content_id)
            self.assertEqual(2, len(writes))
            for path, _old, new in writes:
                path.write_bytes(new)

            catalog_text = (common / "content_catalog.rs").read_text(encoding="utf-8")
            ledger_text = (content / "CONTENT_ID_CATALOG.md").read_text(encoding="utf-8")
            self.assertIn(
                "pub const MONSTER_CAVE_BAT: ContentId = ContentId::from_raw(10_004);",
                catalog_text,
            )
            self.assertIn('"monster.cave_bat" => MONSTER_CAVE_BAT,', catalog_text)
            self.assertIn('MONSTER_CAVE_BAT => "monster.cave_bat",', catalog_text)
            self.assertIn("| `10004` | `monster.cave_bat` | active |", ledger_text)

    def test_filename_uses_authored_id(self):
        self.assertEqual("monster.slime.blue.json", safe_filename("monster.slime.blue"))
        with self.assertRaises(ValueError):
            safe_filename("Monster Bad")


if __name__ == "__main__":
    unittest.main()
