import json
import tempfile
import unittest
from pathlib import Path

from server import (
    load_numeric_catalog,
    find_sprite_manifest_path,
    load_sprite_manifest_document,
    load_sprite_record,
    save_sprite_manifest_document,
    monster_constant_name,
    monster_reserved_ids,
    next_monster_content_id,
    prepare_monster_allocation,
    clone_monster_document,
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

    def test_clone_monster_document_preserves_runtime_values(self):
        source = new_monster_document("monster.slime.red", "Red Slime")
        source["health_max"] = 77.0
        source["movement_speed"] = 3.25
        source["collision_bounds"]["left"] = 0.7
        clone = clone_monster_document(
            source,
            "monster.slime.red.copy",
            "Red Slime Copy",
            "creature.moss_crab",
        )
        self.assertEqual("monster.slime.red.copy", clone["id"])
        self.assertEqual("Red Slime Copy", clone["debug_name"])
        self.assertEqual("creature.moss_crab", clone["sprite"])
        self.assertEqual(77.0, clone["health_max"])
        self.assertEqual(3.25, clone["movement_speed"])
        self.assertEqual(0.7, clone["collision_bounds"]["left"])
        self.assertEqual("monster.slime.red", source["id"])


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
                        "grid_size": [4, 4],
                        "world_size": [1.0, 1.5],
                        "frame_seconds": 0.1,
                        "authored_facing": "right",
                        "clips": {
                            "idle": {"frames": [4, 5], "loop": True},
                            "hit": {"frames": [8, 9], "loop": False},
                            "special_attack_3": {"frames": [12, 13, 14, 15], "loop": False},
                        },
                    }
                ),
                encoding="utf-8",
            )
            items, issues = scan_sprite_manifests(root)
            self.assertEqual([], issues)
            self.assertEqual(["creature.test"], [item["id"] for item in items])
            record = load_sprite_record(root, "creature.test")
            self.assertEqual("creature.test", record["id"])
            self.assertEqual([4, 4], record["grid_size"])
            self.assertEqual([4, 5], record["idle_frames"])


    def test_sprite_scanner_rejects_manifest_without_idle(self):
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
                        "grid_size": [2, 2],
                        "world_size": [1.0, 1.0],
                        "frame_seconds": 0.1,
                        "authored_facing": "right",
                        "clips": {
                            "hit": {"frames": [0], "loop": False},
                            "death": {"frames": [1], "loop": False},
                        },
                    }
                ),
                encoding="utf-8",
            )
            items, issues = scan_sprite_manifests(root)
            self.assertEqual([], items)
            self.assertEqual(1, len(issues))
            self.assertIn("missing required idle clip", issues[0])


    def test_sprite_manifest_save_validates_and_rolls_back(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sprite_dir = root / "Graphic" / "creature" / "test"
            sprite_dir.mkdir(parents=True)
            (sprite_dir / "atlas.png").write_bytes(b"png")
            original = {
                "schema_version": 1,
                "kind": "purgatory_sprite_animation",
                "id": "creature.test",
                "atlas": "atlas.png",
                "frame_size_px": [64, 64],
                "grid_size": [4, 4],
                "world_size": [1.0, 1.0],
                "frame_seconds": 0.1,
                "authored_facing": "right",
                "clips": {
                    "move": {"frames": [0], "loop": True},
                    "idle": {"frames": [0], "loop": True},
                    "attack": {"frames": [0], "loop": False},
                },
            }
            manifest_path = sprite_dir / "manifest.json"
            manifest_path.write_text(json.dumps(original), encoding="utf-8")

            self.assertEqual(manifest_path, find_sprite_manifest_path(root, "creature.test"))
            path, loaded = load_sprite_manifest_document(root, "creature.test")
            self.assertEqual(manifest_path, path)
            self.assertEqual([4, 4], loaded["grid_size"])

            edited = dict(loaded)
            edited["grid_size"] = [6, 4]
            record = save_sprite_manifest_document(root, "creature.test", edited)
            self.assertEqual([6, 4], record["grid_size"])

            invalid = dict(edited)
            invalid["world_size"] = [0, 1]
            with self.assertRaises(ValueError):
                save_sprite_manifest_document(root, "creature.test", invalid)
            persisted = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual([6, 4], persisted["grid_size"])


    def test_monster_allocator_accepts_windows_crlf_catalog(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            common = root / "crates" / "common" / "src"
            common.mkdir(parents=True)
            catalog_path = common / "content_catalog.rs"
            catalog_path.write_bytes(
                (
                    "use crate::ContentId;\r\n"
                    "\r\n"
                    "pub const MONSTER_RED_SLIME: ContentId = ContentId::from_raw(10_001);\r\n"
                    "pub const MONSTER_MOSS_CRAB: ContentId = ContentId::from_raw(10_002);\r\n"
                    "\r\n"
                    "pub fn allocated_id_for_label(label: &str) -> Option<ContentId> {\r\n"
                    "    Some(match label {\r\n"
                    '        "monster.slime.red" => MONSTER_RED_SLIME,\r\n'
                    '        "monster.moss_crab" => MONSTER_MOSS_CRAB,\r\n'
                    "        _ => return None,\r\n"
                    "    })\r\n"
                    "}\r\n"
                    "\r\n"
                    "pub fn label_for_allocated_id(id: ContentId) -> Option<&'static str> {\r\n"
                    "    Some(match id {\r\n"
                    '        MONSTER_RED_SLIME => "monster.slime.red",\r\n'
                    '        MONSTER_MOSS_CRAB => "monster.moss_crab",\r\n'
                    "        _ => return None,\r\n"
                    "    })\r\n"
                    "}\r\n"
                ).encode("utf-8")
            )
            content = root / "content"
            content.mkdir(parents=True)
            (content / "CONTENT_ID_CATALOG.md").write_bytes(
                (
                    "# Content ID Catalog\r\n"
                    "\r\n"
                    "### Monsters (allocation range owned elsewhere)\r\n"
                    "\r\n"
                    "| ID | Label | Status |\r\n"
                    "| ---: | --- | --- |\r\n"
                    "| `10001` | `monster.slime.red` | active |\r\n"
                    "| `10002` | `monster.moss_crab` | active |\r\n"
                    "\r\n"
                    "### NPCs — 20,000–29,999\r\n"
                ).encode("utf-8")
            )

            content_id, writes = prepare_monster_allocation(root, "monster.shroom")
            self.assertEqual(10003, content_id)
            self.assertEqual(2, len(writes))
            catalog_new = next(new for path, _old, new in writes if path == catalog_path).decode("utf-8")
            self.assertIn(
                "pub const MONSTER_SHROOM: ContentId = ContentId::from_raw(10_003);",
                catalog_new,
            )
            self.assertIn('"monster.shroom" => MONSTER_SHROOM,', catalog_new)
            self.assertIn('MONSTER_SHROOM => "monster.shroom",', catalog_new)
            ledger_new = next(
                new
                for path, _old, new in writes
                if path == content / "CONTENT_ID_CATALOG.md"
            ).decode("utf-8")
            self.assertNotIn("\r", catalog_new)
            self.assertNotIn("\r", ledger_new)

    def test_monster_allocator_assigns_next_permanent_id_and_updates_catalog_and_ledger(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            common = root / "crates" / "common" / "src"
            common.mkdir(parents=True)
            (common / "content_catalog.rs").write_text(
                """use crate::ContentId;

pub const MONSTER_RED_SLIME: ContentId = ContentId::from_raw(10_001);
pub const MONSTER_MOSS_CRAB: ContentId = ContentId::from_raw(10_002);
pub const MONSTER_RETIRED_WISP: ContentId = ContentId::from_raw(10_003);

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
