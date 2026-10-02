import shutil
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import server


ROOT = Path(__file__).resolve().parents[2]


class CatalogTests(unittest.TestCase):
    def test_next_id_is_after_the_fixture(self):
        catalog = (ROOT / "crates/common/src/content_catalog.rs").read_text(encoding="utf-8")
        ledger = (ROOT / "content/CONTENT_ID_CATALOG.md").read_text(encoding="utf-8")
        self.assertEqual(server.next_item_id(catalog, ledger), 30000)

    def test_insert_registers_label_and_constant(self):
        catalog = (ROOT / "crates/common/src/content_catalog.rs").read_text(encoding="utf-8")
        updated = server.insert_catalog(catalog, "item.dev.catalog_probe", 30000)
        self.assertIn("ITEM_DEV_CATALOG_PROBE: ContentId = ContentId::from_raw(30_000);", updated)
        self.assertIn('"item.dev.catalog_probe" => ITEM_DEV_CATALOG_PROBE,', updated)
        self.assertIn('ITEM_DEV_CATALOG_PROBE => "item.dev.catalog_probe",', updated)
        with self.assertRaises(RuntimeError):
            server.insert_catalog(updated, "item.dev.sample_scrap", 30017)

    def test_save_accepts_an_existing_equipment_label(self):
        sword = {
            "label": "equipment.debug.practice_sword",
            "category": "equipment",
            "stack_limit": 1,
            "icon": "item.placeholder",
            "equipment_slot": "weapon",
            "display_name": "Practice sword",
            "description": "A practice blade.",
        }
        saved = server.validate_payload(sword, creating=False)
        self.assertFalse(any("label" in error for error in saved))
        created = server.validate_payload(sword, creating=True)
        self.assertIn("label must match item.*", created)
        item = dict(sword, label="item.debug.practice_sword", category="material", stack_limit=20)
        item.pop("equipment_slot")
        self.assertFalse(any("label" in error for error in server.validate_payload(item, creating=False)))

    def test_invalid_icon_and_stack_are_rejected(self):
        errors = server.validate_payload(
            {
                "label": "item.dev.bad",
                "category": "material",
                "stack_limit": 0,
                "icon": "Graphic/items/a.png",
            },
            creating=True,
        )
        self.assertTrue(any("stack" in error for error in errors))
        self.assertTrue(any("icon" in error for error in errors))

    def test_retired_ledger_id_is_not_reused(self):
        catalog = "pub const ITEM_X: ContentId = ContentId::from_raw(30_000);"
        ledger = "| `30000` | `item.retired` | retired |\n| `30015` | `item.dev.sample_scrap` | active |"
        self.assertEqual(server.next_item_id(catalog, ledger), 30001)


class ExpectationTests(unittest.TestCase):
    def test_shared_chart_behavior(self):
        node = shutil.which("node")
        self.assertIsNotNone(node, "node is required to run the shared chart behavior test")
        completed = subprocess.run(
            [node, "tools/test_authoring_chart.mjs"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(0, completed.returncode, completed.stdout + completed.stderr)
        self.assertIn("authoring chart ok", completed.stdout)

    def test_both_labs_use_the_shared_chart(self):
        item_html = (ROOT / "tools/item_lab/web/index.html").read_text(encoding="utf-8")
        mob_html = (ROOT / "tools/mob_lab/web/index.html").read_text(encoding="utf-8")
        item_js = (ROOT / "tools/item_lab/web/app.js").read_text(encoding="utf-8")
        drops = (ROOT / "tools/mob_lab/web/drops.js").read_text(encoding="utf-8")
        self.assertIn('src="/authoring_chart.js"', item_html)
        self.assertIn('src="/authoring_chart.js', mob_html)
        self.assertIn("window.filterItems", item_js)
        self.assertIn("window.renderExpectationChart", item_js)
        self.assertIn("window.dropExpectation", drops)
        self.assertIn("window.renderExpectationChart", drops)


if __name__ == "__main__":
    unittest.main()
