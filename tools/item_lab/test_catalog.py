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
    def expectation(self, bps, low, high, kills):
        p = bps / 10000
        mean = (low + high) / 2
        return kills * p, kills * p * mean

    def test_required_examples(self):
        self.assertEqual(self.expectation(1000, 1, 1, 1000), (100, 100))
        self.assertEqual(self.expectation(1000, 1, 3, 1000), (100, 200))
        self.assertEqual(self.expectation(10000, 2, 2, 100), (100, 200))
        self.assertEqual(self.expectation(0, 1, 1, 1000), (0, 0))

    def test_javascript_uses_the_same_formula(self):
        source = (ROOT / "tools/mob_lab/web/drops.js").read_text(encoding="utf-8")
        self.assertIn("successes: n * p", source)
        self.assertIn("units: n * p * mean", source)


if __name__ == "__main__":
    unittest.main()
