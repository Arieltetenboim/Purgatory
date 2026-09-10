import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import catalog


class NpcLabCatalogTests(unittest.TestCase):
    def test_animation_catalog_scans_nested_anim_files_by_stem(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            animation_root = root / "content" / "shared" / "animations"
            (animation_root / "dev").mkdir(parents=True)
            (animation_root / "npc").mkdir(parents=True)
            (animation_root / "dev" / "wave.anim").write_text("schema_version 1\n", encoding="utf-8")
            (animation_root / "npc" / "point.anim").write_text("schema_version 1\n", encoding="utf-8")
            (animation_root / "npc" / "ignore.txt").write_text("x", encoding="utf-8")

            items = catalog.list_catalog(root, "animation")

            self.assertEqual([item["id"] for item in items], ["point", "wave"])
            self.assertEqual(
                {item["path"] for item in items},
                {
                    "content/shared/animations/dev/wave.anim",
                    "content/shared/animations/npc/point.anim",
                },
            )

    def test_duplicate_animation_stems_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            animation_root = root / "content" / "shared" / "animations"
            (animation_root / "a").mkdir(parents=True)
            (animation_root / "b").mkdir(parents=True)
            (animation_root / "a" / "wave.anim").write_text("x", encoding="utf-8")
            (animation_root / "b" / "wave.anim").write_text("x", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "duplicate id"):
                catalog.list_catalog(root, "animation")

    def test_json_id_mode_supports_future_item_catalog_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            item_root = root / "content" / "shared" / "items"
            item_root.mkdir(parents=True)
            (item_root / "practice_sword.json").write_text(
                json.dumps({"id": "item.practice_sword"}), encoding="utf-8"
            )
            source = catalog.CatalogSource(
                relative_root="content/shared/items", suffix=".json", id_mode="json_id"
            )
            original = catalog.CATALOG_SOURCES.get("test_item")
            catalog.CATALOG_SOURCES["test_item"] = source
            try:
                items = catalog.list_catalog(root, "test_item")
            finally:
                if original is None:
                    del catalog.CATALOG_SOURCES["test_item"]
                else:
                    catalog.CATALOG_SOURCES["test_item"] = original

            self.assertEqual(items[0]["id"], "item.practice_sword")

    def test_unknown_catalog_kind_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "Unsupported catalog kind"):
                catalog.list_catalog(Path(tmp), "unknown")


if __name__ == "__main__":
    unittest.main()
