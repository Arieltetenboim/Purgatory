"""Item Lab and Mob Lab save recovery, revision, and chart checks."""

from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))

from authoring_catalog import CatalogWriteLock
from authoring_save import AuthoringOperation, AuthoringRepair, recover_authoring


def load_item_lab():
    spec = importlib.util.spec_from_file_location("item_lab_server", TOOLS / "item_lab" / "server.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


LAB = load_item_lab()


def payload(label: str, **extra) -> dict:
    body = {
        "label": label,
        "category": "material",
        "stack_limit": 20,
        "drop_requires_confirmation": False,
        "display_name": "Sample",
        "description": "A sample",
        "icon": "item.placeholder",
        "notes": "",
        "tags": ["scrap"],
        "revision": None,
    }
    body.update(extra)
    return body


def seed_item(root: Path, label: str, content_id: int, **extra) -> dict:
    body = payload(label, **extra)
    for path, text in LAB.planned_item_files(root, body, content_id):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return LAB.load_item(root, LAB.items_dir(root) / f"{label}.json")


class SaveTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmpdir = tempfile.TemporaryDirectory()
        self.root = Path(self._tmpdir.name)
        self._validate = LAB.validate_pack
        LAB.validate_pack = lambda root: None

    def tearDown(self) -> None:
        LAB.validate_pack = self._validate
        self._tmpdir.cleanup()

    def test_concurrent_saves_from_one_revision_conflict(self) -> None:
        current = seed_item(self.root, "item.sample", 30000)
        results: list[object] = []
        barrier = threading.Barrier(2)

        def attempt(name: str) -> None:
            body = payload("item.sample", display_name=name, revision=current["revision"])
            barrier.wait()
            try:
                results.append(LAB.commit_item_save(self.root, body)["item"]["display_name"])
            except LAB.AuthoringConflict as error:
                results.append(error)

        threads = [threading.Thread(target=attempt, args=(name,)) for name in ("One", "Two")]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(1, sum(isinstance(result, str) for result in results))
        self.assertEqual(1, sum(isinstance(result, LAB.AuthoringConflict) for result in results))
        saved = LAB.load_item(self.root, LAB.items_dir(self.root) / "item.sample.json")
        self.assertIn(saved["display_name"], {"One", "Two"})

    def test_notes_only_change_invalidates_older_notes_draft(self) -> None:
        current = seed_item(self.root, "item.sample", 30000, notes="first")
        old = current["revision"]
        LAB.commit_item_save(self.root, payload("item.sample", notes="second", revision=old))
        with self.assertRaises(LAB.AuthoringConflict):
            LAB.commit_item_save(self.root, payload("item.sample", notes="third", revision=old))
        saved = LAB.load_item(self.root, LAB.items_dir(self.root) / "item.sample.json")
        self.assertEqual("second", saved["notes"])

    def test_validation_failure_restores_equipment_and_unrelated_file(self) -> None:
        current = seed_item(
            self.root,
            "item.helm",
            30020,
            category="equipment",
            stack_limit=1,
            equipment_slot="headwear",
            drop_requires_confirmation=True,
        )
        equip = LAB.equipment_path(self.root, 30020)
        assert equip is not None
        merged = json.loads(equip.read_text(encoding="utf-8"))
        merged["worn_visual"] = "kept"
        equip.write_text(json.dumps(merged, indent=2) + "\n", encoding="utf-8")
        current = LAB.load_item(self.root, LAB.items_dir(self.root) / "item.helm.json")
        unrelated = self.root / "content" / "shared" / "items" / "unrelated.txt"
        unrelated.write_text("leave me", encoding="utf-8")
        before = equip.read_bytes()

        def fail(_root: Path) -> None:
            raise RuntimeError("validator rejected the pack")

        LAB.validate_pack = fail
        with self.assertRaises(RuntimeError):
            LAB.commit_item_save(
                self.root,
                payload(
                    "item.helm",
                    category="equipment",
                    stack_limit=1,
                    equipment_slot="headwear",
                    display_name="Changed",
                    revision=current["revision"],
                ),
            )
        self.assertEqual(before, equip.read_bytes())
        self.assertEqual("leave me", unrelated.read_text(encoding="utf-8"))
        self.assertNotIn("Changed", (LAB.items_dir(self.root) / "item.helm.json").read_text(encoding="utf-8"))

    def test_injected_publication_boundary_restores_every_touched_file(self) -> None:
        gameplay = self.root / "content" / "shared" / "items" / "item.sample.json"
        presentation = self.root / "content" / "shared" / "item_presentation" / "item.sample.json"
        notes = self.root / "content" / "authoring" / "item_notes" / "30000.json"
        for path, text in (
            (gameplay, "game-before\n"),
            (presentation, "present-before\n"),
            (notes, "notes-before\n"),
        ):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        unrelated = self.root / "content" / "shared" / "items" / "other.txt"
        unrelated.write_text("other", encoding="utf-8")
        for fail_after in (0, 1, 2):
            operation = AuthoringOperation(self.root)
            operation.stage(gameplay, b"game-after\n")
            operation.stage(presentation, b"present-after\n")
            operation.stage(notes, b"notes-after\n")
            with self.assertRaises(RuntimeError):
                operation.publish(fail_after=fail_after)
            operation.rollback()
            self.assertEqual("game-before\n", gameplay.read_text(encoding="utf-8"))
            self.assertEqual("present-before\n", presentation.read_text(encoding="utf-8"))
            self.assertEqual("notes-before\n", notes.read_text(encoding="utf-8"))
            self.assertEqual("other", unrelated.read_text(encoding="utf-8"))
            self.assertFalse((self.root / "content" / ".authoring-recovery").exists())

    def test_process_exit_during_publish_recovers_on_restart(self) -> None:
        gameplay = self.root / "content" / "shared" / "items" / "item.sample.json"
        presentation = self.root / "content" / "shared" / "item_presentation" / "item.sample.json"
        gameplay.parent.mkdir(parents=True)
        presentation.parent.mkdir(parents=True)
        gameplay.write_text("before-game\n", encoding="utf-8")
        presentation.write_text("before-present\n", encoding="utf-8")
        script = r"""
import os, sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from authoring_save import AuthoringOperation
root = Path(sys.argv[2])
operation = AuthoringOperation(root, "crash-op")
operation.stage(root / "content/shared/items/item.sample.json", b"after-game\n")
operation.stage(root / "content/shared/item_presentation/item.sample.json", b"after-present\n")
operation.publish()
os._exit(77)
"""
        completed = subprocess.run(
            [sys.executable, "-c", script, str(TOOLS), str(self.root)],
            check=False,
        )
        self.assertEqual(77, completed.returncode)
        self.assertEqual("after-game\n", gameplay.read_text(encoding="utf-8"))
        notes = recover_authoring(self.root)
        self.assertTrue(any("restored" in note for note in notes))
        self.assertEqual("before-game\n", gameplay.read_text(encoding="utf-8"))
        self.assertEqual("before-present\n", presentation.read_text(encoding="utf-8"))

    def test_external_bytes_fail_closed(self) -> None:
        target = self.root / "content" / "shared" / "items" / "item.sample.json"
        target.parent.mkdir(parents=True)
        target.write_text("before\n", encoding="utf-8")
        operation = AuthoringOperation(self.root, "external")
        operation.stage(target, b"after\n")
        operation.publish()
        target.write_text("someone-else\n", encoding="utf-8")
        with self.assertRaises(AuthoringRepair):
            recover_authoring(self.root)
        self.assertEqual("someone-else\n", target.read_text(encoding="utf-8"))

    def test_finish_is_the_commit_and_retry_does_not_allocate_again(self) -> None:
        self._copy_catalog()
        original_run = LAB.subprocess.run

        def fake_run(*_args, **_kwargs):
            class Result:
                returncode = 0
                stderr = ""
                stdout = ""
            return Result()

        LAB.subprocess.run = fake_run
        try:
            first = LAB.commit_item_create(self.root, payload("item.created_once"))
            second = LAB.commit_item_create(self.root, payload("item.created_once"))
        finally:
            LAB.subprocess.run = original_run
        self.assertEqual(first["content_id"], second["content_id"])
        self.assertTrue(second.get("idempotent"))
        catalog = (self.root / "crates" / "common" / "src" / "content_catalog.rs").read_text(encoding="utf-8")
        self.assertEqual(1, catalog.count("pub const ITEM_CREATED_ONCE"))

    def test_stack_reduction_and_bad_icon_do_not_mutate(self) -> None:
        current = seed_item(self.root, "item.sample", 30000, stack_limit=20)
        before = (LAB.items_dir(self.root) / "item.sample.json").read_bytes()
        with self.assertRaises(LAB.AuthoringRejected):
            LAB.commit_item_save(
                self.root,
                payload("item.sample", stack_limit=5, revision=current["revision"]),
            )
        self.assertEqual(before, (LAB.items_dir(self.root) / "item.sample.json").read_bytes())
        with self.assertRaises(LAB.AuthoringRejected):
            LAB.commit_icon(self.root, "item.sample", b"not a png")
        self.assertFalse((LAB.icons_dir(self.root) / "item.sample.png").exists())

    def test_lock_timeout_then_success(self) -> None:
        held = threading.Event()
        release = threading.Event()

        def holder() -> None:
            with CatalogWriteLock(self.root, timeout_s=2):
                held.set()
                release.wait(2)

        thread = threading.Thread(target=holder)
        thread.start()
        self.assertTrue(held.wait(2))
        with self.assertRaises(TimeoutError):
            with CatalogWriteLock(self.root, timeout_s=0.2):
                pass
        release.set()
        thread.join()
        with CatalogWriteLock(self.root, timeout_s=2):
            pass

    def test_retired_id_is_not_reused(self) -> None:
        catalog = "ContentId::from_raw(30_001);"
        ledger = "| `30000` | `item.old` | retired |"
        self.assertEqual(30002, LAB.next_item_id(catalog, ledger))

    def test_extra_authored_field_survives_save(self) -> None:
        current = seed_item(self.root, "item.sample", 30000)
        path = LAB.items_dir(self.root) / "item.sample.json"
        document = json.loads(path.read_text(encoding="utf-8"))
        document["future_field"] = "kept"
        path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
        current = LAB.load_item(self.root, path)
        LAB.commit_item_save(
            self.root,
            payload("item.sample", display_name="Renamed", revision=current["revision"]),
        )
        saved = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual("kept", saved["future_field"])
        self.assertEqual("item.sample", saved["label"])
        presentation = json.loads(
            (LAB.presentation_dir(self.root) / "item.sample.json").read_text(encoding="utf-8")
        )
        self.assertEqual("Renamed", presentation["display_name"])

    def test_concurrent_catalog_allocation_keeps_distinct_ids(self) -> None:
        self._copy_catalog()
        spec = importlib.util.spec_from_file_location("mob_lab_server", TOOLS / "mob_lab" / "server.py")
        mob = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mob)
        ids: list[int] = []
        errors: list[BaseException] = []

        def allocate_item() -> None:
            try:
                with CatalogWriteLock(self.root):
                    catalog_path = LAB.catalog_rs(self.root)
                    ledger_path = LAB.catalog_md(self.root)
                    catalog = catalog_path.read_text(encoding="utf-8")
                    ledger = ledger_path.read_text(encoding="utf-8")
                    content_id = LAB.next_item_id(catalog, ledger)
                    catalog_path.write_text(
                        LAB.insert_catalog(catalog.replace("\r\n", "\n"), "item.alloc_a", content_id),
                        encoding="utf-8",
                        newline="\n",
                    )
                    ledger_path.write_text(
                        LAB.insert_ledger(ledger.replace("\r\n", "\n"), "item.alloc_a", content_id),
                        encoding="utf-8",
                        newline="\n",
                    )
                    ids.append(content_id)
            except BaseException as error:
                errors.append(error)

        def allocate_monster() -> None:
            try:
                with CatalogWriteLock(self.root):
                    content_id, writes = mob.prepare_monster_allocation(self.root, "monster.alloc_b")
                    for target, _old, new in writes:
                        target.write_bytes(new)
                    ids.append(content_id)
            except BaseException as error:
                errors.append(error)

        threads = [threading.Thread(target=allocate_item), threading.Thread(target=allocate_monster)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual([], errors)
        self.assertEqual(2, len(set(ids)))
        catalog = LAB.catalog_rs(self.root).read_text(encoding="utf-8")
        self.assertIn("item.alloc_a", catalog)
        self.assertIn("monster.alloc_b", catalog)
        self.assertIn(f"| `{ids[0] if ids[0] >= 30000 else ids[1]}` | `item.alloc_a` | active |", LAB.catalog_md(self.root).read_text(encoding="utf-8"))

    def test_chart_module(self) -> None:
        node = shutil.which("node")
        self.assertIsNotNone(node)
        completed = subprocess.run(
            [node, "tools/test_authoring_chart.mjs"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(0, completed.returncode, completed.stdout + completed.stderr)

    def _copy_catalog(self) -> None:
        for relative in (
            "crates/common/src/content_catalog.rs",
            "crates/common/src/lib.rs",
            "content/CONTENT_ID_CATALOG.md",
        ):
            destination = self.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)


if __name__ == "__main__":
    unittest.main()
