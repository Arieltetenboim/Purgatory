import json
import binascii
import struct
import tempfile
import unittest
from unittest import mock
from pathlib import Path
import zlib

import server
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
    sprite_public_record,
    validate_monster_document,
)


def tiny_png(width: int, height: int) -> bytes:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", binascii.crc32(kind + payload) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(
            b"IDAT",
            zlib.compress(
                b"".join(b"\x00" + b"\x00\x00\x00\x00" * width for _ in range(height))
            ),
        )
        + chunk(b"IEND", b"")
    )


class MobLabContractTests(unittest.TestCase):
    @staticmethod
    def v2_manifest() -> dict:
        return {
            "schema_version": 2,
            "kind": "purgatory_sprite_animation",
            "id": "creature.goblin_archer",
            "atlas": "atlas.png",
            "pixels_per_unit": 64.0,
            "authored_facing": "right",
            "frames": [
                {
                    "rect_px": [0, 0, 4, 6],
                    "origin_px": [7, -2],
                    "sockets": {
                        "hand": [-3, 8],
                        "projectile_spawn": [5, -1],
                    },
                },
                {
                    "rect_px": [4, 0, 3, 8],
                    "origin_px": [1, 4],
                    "sockets": {},
                },
            ],
            "clips": {
                "idle": {
                    "loop": True,
                    "steps": [
                        {"frame": 0, "duration_ms": 100},
                        {"frame": 1, "duration_ms": 150},
                    ],
                    "annotations": [
                        {"name": "aim", "step": 0, "offset_ms": 0, "socket": "hand"},
                        {"name": "release", "step": 1, "offset_ms": 20},
                    ],
                },
                "special.attack-1": {
                    "loop": False,
                    "steps": [{"frame": 1, "duration_ms": 200}],
                },
            },
            "editor_note": "preserve this optional field",
        }

    @staticmethod
    def write_v2_fixture(root: Path, manifest: dict) -> Path:
        sprite_dir = root / "Graphic" / "creature" / "goblin"
        sprite_dir.mkdir(parents=True)
        (sprite_dir / "atlas.png").write_bytes(tiny_png(8, 8))
        path = sprite_dir / "manifest.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        return path

    def test_valid_v2_manifest_is_discovered_and_preserves_authored_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_v2_fixture(root, self.v2_manifest())
            items, issues = scan_sprite_manifests(root)
            self.assertEqual([], issues)
            self.assertEqual(["creature.goblin_archer"], [item["id"] for item in items])
            record = load_sprite_record(root, "creature.goblin_archer")
            self.assertEqual([8, 8], record["atlas_size_px"])
            self.assertEqual(self.v2_manifest()["frames"], record["frames"])
            self.assertEqual(
                ["idle", "special.attack-1"], list(record["clips"])
            )
            self.assertEqual(
                self.v2_manifest()["editor_note"],
                sprite_public_record(record)["editor_note"],
            )

    def test_v2_accepts_variable_frames_origins_and_signed_sockets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.write_v2_fixture(root, self.v2_manifest())
            item = load_sprite_record(root, "creature.goblin_archer")
            self.assertEqual([4, 6], item["frames"][0]["rect_px"][2:])
            self.assertEqual([7, -2], item["frames"][0]["origin_px"])
            self.assertEqual([-3, 8], item["frames"][0]["sockets"]["hand"])

    def test_v2_rejects_invalid_or_overflowing_rectangles(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = self.v2_manifest()
            manifest["frames"][0]["rect_px"] = [6, 4, 3, 5]
            self.write_v2_fixture(root, manifest)
            items, issues = scan_sprite_manifests(root)
            self.assertEqual([], items)
            self.assertIn("does not fit inside atlas", issues[0])

    def test_v2_rejects_invalid_pixels_per_unit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = self.v2_manifest()
            manifest["pixels_per_unit"] = True
            self.write_v2_fixture(root, manifest)
            _items, issues = scan_sprite_manifests(root)
            self.assertIn("pixels_per_unit", issues[0])

    def test_v2_rejects_invalid_frame_references_and_durations(self):
        for mutation, expected in (
            (lambda doc: doc["clips"]["idle"]["steps"][0].update(frame=2), "frame is invalid"),
            (lambda doc: doc["clips"]["idle"]["steps"][0].update(duration_ms=0), "positive integer"),
            (lambda doc: doc["clips"]["idle"]["steps"][0].update(duration_ms=-1), "positive integer"),
        ):
            with self.subTest(expected=expected), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                manifest = self.v2_manifest()
                mutation(manifest)
                self.write_v2_fixture(root, manifest)
                _items, issues = scan_sprite_manifests(root)
                self.assertIn(expected, issues[0])

    def test_v2_rejects_invalid_annotation_step_offset_and_socket(self):
        mutations = (
            (lambda doc: doc["clips"]["idle"]["annotations"][0].update(step=2), "step is invalid"),
            (lambda doc: doc["clips"]["idle"]["annotations"][0].update(offset_ms=100), "smaller than step duration"),
            (lambda doc: doc["clips"]["idle"]["annotations"][0].update(socket="missing"), "missing from referenced frame"),
        )
        for mutation, expected in mutations:
            with self.subTest(expected=expected), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                manifest = self.v2_manifest()
                mutation(manifest)
                self.write_v2_fixture(root, manifest)
                _items, issues = scan_sprite_manifests(root)
                self.assertIn(expected, issues[0])

    def test_v2_requires_idle_and_missing_optional_fields_use_defaults(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = self.v2_manifest()
            manifest["clips"]["special.attack-1"].pop("annotations", None)
            manifest["frames"][1].pop("sockets")
            self.write_v2_fixture(root, manifest)
            items, issues = scan_sprite_manifests(root)
            self.assertEqual([], issues)
            self.assertEqual({}, items[0]["frames"][1].get("sockets", {}))
            self.assertEqual([], items[0]["clips"]["special.attack-1"].get("annotations", []))

            del manifest["clips"]["idle"]
            (root / "Graphic" / "creature" / "goblin" / "manifest.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            _items, issues = scan_sprite_manifests(root)
            self.assertIn("missing required idle clip", issues[0])

    def test_v2_invalid_save_preserves_previous_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = self.write_v2_fixture(root, self.v2_manifest())
            _path, loaded = load_sprite_manifest_document(root, "creature.goblin_archer")
            invalid = json.loads(json.dumps(loaded))
            invalid["clips"]["idle"]["steps"][0]["duration_ms"] = 0
            with self.assertRaises(ValueError):
                save_sprite_manifest_document(root, "creature.goblin_archer", invalid)
            self.assertEqual(loaded, json.loads(path.read_text(encoding="utf-8")))

    def test_v2_fields_survive_save_and_reload_without_normalization(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = self.v2_manifest()
            self.write_v2_fixture(root, original)
            edited = json.loads(json.dumps(original))
            edited["frames"][0]["sockets"]["custom_socket"] = [-8, 13]
            edited["clips"]["idle"]["annotations"].append(
                {"name": "late.marker", "step": 1, "offset_ms": 149}
            )
            saved, record = save_sprite_manifest_document(
                root, "creature.goblin_archer", edited
            )
            self.assertEqual(edited, saved)
            self.assertEqual(edited, record["document"])
            self.assertEqual(edited, json.loads(
                (root / "Graphic" / "creature" / "goblin" / "manifest.json").read_text(
                    encoding="utf-8"
                )
            ))

    def test_default_document_matches_schema_v4_contract(self):
        doc = new_monster_document(
            "monster.test.creature", "Test Creature", "creature.moss_crab"
        )
        self.assertEqual([], validate_monster_document(doc))
        self.assertEqual(4, doc["schema_version"])
        self.assertEqual("creature.moss_crab", doc["sprite"])
        self.assertEqual(
            {"left": 0.4, "right": 0.4, "bottom": 0.6, "top": 0.6},
            doc["collision_bounds"],
        )
        self.assertEqual("when_attacked", doc["behavior"]["aggro"])

    def test_clone_monster_document_preserves_runtime_values(self):
        source = new_monster_document(
            "monster.test.source", "Source Monster", "creature.shroom"
        )
        source["health_max"] = 77.0
        source["movement_speed"] = 3.25
        source["collision_bounds"]["left"] = 0.7
        clone = clone_monster_document(
            source,
            "monster.test.copy",
            "Monster Copy",
            "creature.moss_crab",
        )
        self.assertEqual("monster.test.copy", clone["id"])
        self.assertEqual("Monster Copy", clone["debug_name"])
        self.assertEqual("creature.moss_crab", clone["sprite"])
        self.assertEqual(77.0, clone["health_max"])
        self.assertEqual(3.25, clone["movement_speed"])
        self.assertEqual(0.7, clone["collision_bounds"]["left"])
        self.assertEqual("monster.test.source", source["id"])


    def test_invalid_runtime_values_are_rejected_before_save(self):
        doc = new_monster_document(
            "monster.test.invalid", "Invalid Monster", "creature.moss_crab"
        )
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
            self.assertEqual(["idle", "hit", "special_attack_3"], list(record["clips"]))


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
            saved, record = save_sprite_manifest_document(root, "creature.test", edited)
            self.assertEqual(edited, saved)
            self.assertEqual([6, 4], record["grid_size"])
            self.assertEqual(
                ["move", "idle", "attack"], list(saved["clips"])
            )

            invalid = dict(edited)
            invalid["world_size"] = [0, 1]
            with self.assertRaises(ValueError):
                save_sprite_manifest_document(root, "creature.test", invalid)
            persisted = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual([6, 4], persisted["grid_size"])

    def test_monster_save_reload_preserves_valid_document_and_rejects_invalid(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            definitions = root / "content" / "definitions" / "monsters"
            definitions.mkdir(parents=True)
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
                        "grid_size": [1, 1],
                        "world_size": [1, 1],
                        "frame_seconds": 0.1,
                        "authored_facing": "right",
                        "clips": {"idle": {"frames": [0], "loop": True}},
                    }
                ),
                encoding="utf-8",
            )
            path = definitions / "monster.test.json"
            original = new_monster_document("monster.test", "Test")
            original["sprite"] = "creature.test"
            path.write_text(json.dumps(original), encoding="utf-8")
            edited = json.loads(json.dumps(original))
            edited["debug_name"] = "Edited"
            edited["behavior"]["home_leash_radius"] = 9.5
            handler = object.__new__(server.MobLabHandler)
            handler.repo_root = root

            with mock.patch("server.validate_runtime_pack", return_value=(True, "")):
                ok, errors, _ = handler._save_candidate(path, edited)
            self.assertTrue(ok)
            self.assertEqual([], errors)
            self.assertEqual(edited, json.loads(path.read_text(encoding="utf-8")))

            invalid = json.loads(json.dumps(edited))
            invalid["health_max"] = 0
            with mock.patch("server.validate_runtime_pack") as validator:
                ok, errors, _ = handler._save_candidate(path, invalid)
            validator.assert_not_called()
            self.assertFalse(ok)
            self.assertTrue(errors)
            self.assertEqual(edited, json.loads(path.read_text(encoding="utf-8")))

    def test_atomic_write_failure_removes_temporary_file_and_preserves_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.json"
            path.write_bytes(b'{"valid":true}\n')
            with mock.patch("server.os.replace", side_effect=OSError("replace failed")):
                with self.assertRaises(OSError):
                    server.atomic_write(path, b'{"valid":false}\n')
            self.assertEqual(b'{"valid":true}\n', path.read_bytes())
            self.assertEqual([], list(path.parent.glob(f".{path.name}.*.tmp")))


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
