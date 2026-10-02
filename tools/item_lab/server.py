"""Local Item Lab. Edits canonical item content; no database and no second registry."""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from authoring_catalog import CatalogWriteLock
from authoring_save import (
    AuthoringConflict,
    AuthoringOperation,
    AuthoringRepair,
    recover_authoring,
)

HOST = "127.0.0.1"
PORT = 8767
BUILD = "item-lab-v1"
SCHEMA_VERSION = 3
PRESENTATION_SCHEMA = 2
EQUIPMENT_SCHEMA = 2
CATEGORIES = ("equipment", "consumable", "material", "tool", "misc")
SLOTS = ("headwear", "bodywear", "pants", "gloves", "boots", "weapon")
LABEL_RE = re.compile(r"^item\.[a-z0-9][a-z0-9._-]*$")
VISUAL_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
ITEM_MIN = 30_000
ITEM_MAX = 39_999
ICON_PX = 32


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def items_dir(root: Path) -> Path:
    return root / "content" / "shared" / "items"


def presentation_dir(root: Path) -> Path:
    return root / "content" / "shared" / "item_presentation"


def equipment_dir(root: Path) -> Path:
    return root / "content" / "shared" / "equipment"


def notes_dir(root: Path) -> Path:
    return root / "content" / "authoring" / "item_notes"


def icons_dir(root: Path) -> Path:
    return root / "Graphic" / "items"


def catalog_rs(root: Path) -> Path:
    return root / "crates" / "common" / "src" / "content_catalog.rs"


def catalog_md(root: Path) -> Path:
    return root / "content" / "CONTENT_ID_CATALOG.md"


def common_lib(root: Path) -> Path:
    return root / "crates" / "common" / "src" / "lib.rs"


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def atomic_write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(text, encoding="utf-8", newline="\n")
    os.replace(tmp, path)


def item_revision(root: Path, path: Path, content_id: int) -> str:
    presentation_path = presentation_dir(root) / path.name
    notes_path = notes_dir(root) / f"{content_id}.json"
    equip = equipment_path(root, content_id)
    return revision_of(
        path.read_text(encoding="utf-8") if path.exists() else "<absent-gameplay>",
        presentation_path.read_text(encoding="utf-8") if presentation_path.exists() else "<absent-presentation>",
        equip.read_text(encoding="utf-8") if equip is not None else "<absent-equipment>",
        notes_path.read_text(encoding="utf-8") if notes_path.exists() else "<absent-notes>",
    )


def equipment_path(root: Path, content_id: int) -> Path | None:
    for path in equipment_dir(root).glob("*.json"):
        try:
            if read_json(path).get("id") == content_id:
                return path
        except (OSError, json.JSONDecodeError):
            continue
    return None


def revision_of(*parts: str) -> str:
    digest = hashlib.sha256()
    for part in parts:
        digest.update(part.encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def png_size(data: bytes) -> tuple[int, int, int]:
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("file is not a PNG")
    if data[12:16] != b"IHDR":
        raise ValueError("PNG is missing IHDR")
    width = int.from_bytes(data[16:20], "big")
    height = int.from_bytes(data[20:24], "big")
    color = data[25]
    return width, height, color


def item_files(root: Path) -> list[Path]:
    return sorted(items_dir(root).glob("*.json"))


def load_item(root: Path, path: Path) -> dict:
    doc = read_json(path)
    content_id = int(doc["id"])
    presentation_path = presentation_dir(root) / path.name
    presentation = read_json(presentation_path) if presentation_path.exists() else {}
    equipment = equipment_document(root, content_id)
    notes_path = notes_dir(root) / f"{content_id}.json"
    notes = read_json(notes_path) if notes_path.exists() else {"notes": "", "tags": []}
    return {
        "path": str(path.relative_to(root)).replace("\\", "/"),
        "content_id": content_id,
        "label": doc.get("label", ""),
        "category": doc.get("category", ""),
        "stack_limit": doc.get("stack_limit", 1),
        "drop_requires_confirmation": bool(doc.get("drop_requires_confirmation", False)),
        "display_name": presentation.get("display_name", ""),
        "description": presentation.get("description", ""),
        "icon": presentation.get("icon", ""),
        "equipment_slot": None if equipment is None else equipment.get("equipment_slot"),
        "notes": notes.get("notes", ""),
        "tags": notes.get("tags", []),
        "revision": item_revision(root, path, content_id),
        "icon_file": (icons_dir(root) / f"{presentation.get('icon', '')}.png").is_file(),
    }


def list_items(root: Path) -> list[dict]:
    rows = []
    for path in item_files(root):
        try:
            rows.append(load_item(root, path))
        except (OSError, json.JSONDecodeError, KeyError, ValueError):
            continue
    return rows


def equipment_document(root: Path, content_id: int) -> dict | None:
    for path in equipment_dir(root).glob("*.json"):
        try:
            doc = read_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        if doc.get("id") == content_id:
            return doc
    return None


def where_used(root: Path, content_id: int, label: str) -> dict:
    monsters = []
    for path in (root / "content" / "definitions" / "monsters").glob("*.json"):
        try:
            doc = read_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        hits = [
            entry
            for entry in doc.get("drops") or []
            if int(entry.get("item", -1)) == content_id
        ]
        if hits:
            monsters.append({"id": doc.get("id"), "path": path.name, "drops": hits})
    dialogue = []
    npc_root = root / "content" / "authoring" / "npcs"
    if npc_root.exists():
        for path in npc_root.rglob("*.json"):
            text = path.read_text(encoding="utf-8")
            if label and label in text or re.search(rf"\b{content_id}\b", text):
                dialogue.append(str(path.relative_to(root)).replace("\\", "/"))
    equipment = []
    for path in equipment_dir(root).glob("*.json"):
        try:
            doc = read_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        if doc.get("id") == content_id:
            equipment.append(path.name)
    return {"monsters": monsters, "dialogue": dialogue, "equipment": equipment}


def validate_payload(payload: dict, *, creating: bool) -> list[str]:
    errors = []
    label = str(payload.get("label", ""))
    if not LABEL_RE.fullmatch(label):
        errors.append("label must match item.*")
    if payload.get("category") not in CATEGORIES:
        errors.append("category must be one of the five inventory categories")
    try:
        stack = int(payload.get("stack_limit"))
    except (TypeError, ValueError):
        stack = 0
    if stack < 1:
        errors.append("stack_limit must be a positive integer")
    if payload.get("category") == "equipment" and stack != 1:
        errors.append("equipment stack_limit must be 1")
    icon = str(payload.get("icon", ""))
    if not VISUAL_RE.fullmatch(icon) or any(token in icon for token in ("/", "\\", ".png", ".jpg")):
        errors.append("icon must be a visual key, not a path")
    if payload.get("category") == "equipment" and payload.get("equipment_slot") not in SLOTS:
        errors.append("equipment_slot is required for equipment")
    if creating and payload.get("category") != "equipment" and payload.get("equipment_slot"):
        errors.append("only equipment has an equipment slot")
    return errors


def next_item_id(catalog: str, ledger: str) -> int:
    used = {
        int(raw.replace("_", ""))
        for raw in re.findall(r"ContentId::from_raw\(([0-9_]+)\)", catalog)
    }
    used.update(int(raw) for raw in re.findall(r"`(3\d{4})`", ledger))
    candidate = ITEM_MIN
    while candidate in used:
        candidate += 1
        if candidate > ITEM_MAX:
            raise RuntimeError("item id range is exhausted")
    if not ITEM_MIN <= candidate <= ITEM_MAX:
        raise RuntimeError("item id range is exhausted")
    return candidate


def const_name(label: str) -> str:
    name = "ITEM_" + re.sub(r"[^A-Za-z0-9]+", "_", label.removeprefix("item.")).strip("_").upper()
    if name == "ITEM_":
        raise RuntimeError("item label must contain a name after item.")
    return name


def raw_id(content_id: int) -> str:
    return f"{content_id // 1000}_{content_id % 1000:03d}"


def _insert_after(text: str, matches: list[re.Match[str]], line: str, missing: str) -> str:
    if not matches:
        raise RuntimeError(missing)
    last = matches[-1]
    if line in text:
        raise RuntimeError("catalog entry already exists")
    return text[: last.end()] + "\n" + line + text[last.end() :]


def insert_catalog(catalog: str, label: str, content_id: int) -> str:
    const = const_name(label)
    if re.search(rf"\b{re.escape(const)}\b", catalog):
        raise RuntimeError(f"catalog constant {const} already exists")
    if f'"{label}"' in catalog:
        raise RuntimeError("label is already registered")
    catalog = _insert_after(
        catalog,
        list(re.finditer(r"^pub const ITEM_[A-Z0-9_]+: ContentId = ContentId::from_raw\([0-9_]+\);$", catalog, re.M)),
        f"pub const {const}: ContentId = ContentId::from_raw({raw_id(content_id)});",
        "item constant block was not found",
    )
    catalog = _insert_after(
        catalog,
        list(re.finditer(r'^ +"item\.[^"]+" => ITEM_[A-Z0-9_]+,$', catalog, re.M)),
        f'        "{label}" => {const},',
        "item label block was not found",
    )
    catalog = _insert_after(
        catalog,
        list(re.finditer(r'^ +ITEM_[A-Z0-9_]+ => "item\.[^"]+",$', catalog, re.M)),
        f'        {const} => "{label}",',
        "item reverse-label block was not found",
    )
    marker = "ITEM_CLOTH_CAP,"
    start = catalog.find(marker)
    end = catalog.find("] {", start)
    if start < 0 or end < 0:
        raise RuntimeError("item kind test was not found")
    addition = f"            {const},\n"
    if addition not in catalog:
        catalog = catalog[:end] + addition + catalog[end:]
    return catalog


def insert_lib_export(text: str, label: str) -> str:
    const = const_name(label)
    start = text.find("pub use content_catalog::{")
    end = text.find("};", start)
    if start < 0 or end < 0:
        raise RuntimeError("common lib item exports were not found")
    block = text[start:end]
    if const in block:
        return text
    map_at = block.find("MAP1,")
    if map_at < 0:
        raise RuntimeError("common lib item exports were not found")
    block = block[:map_at] + f"{const}, " + block[map_at:]
    return text[:start] + block + text[end:]


def insert_ledger(ledger: str, label: str, content_id: int) -> str:
    if f"| `{content_id}` |" in ledger:
        raise RuntimeError("ledger id already exists")
    rows = list(re.finditer(r"^\| `3\d{4}` \| `[^`]+` \| [^|]+\|$", ledger, re.M))
    if not rows:
        raise RuntimeError("item ledger section was not found")
    last = rows[-1]
    row = f"| `{content_id}` | `{label}` | active |"
    return ledger[: last.end()] + "\n" + row + ledger[last.end() :]


def validate_pack(root: Path) -> None:
    completed = subprocess.run(
        ["cargo", "run", "-q", "-p", "purgatory-content-validator", "--", str(root / "content")],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout or "content validation failed").strip()
        raise RuntimeError(detail[-2000:])


def _merged(path: Path, updates: dict) -> str:
    current = read_json(path) if path.exists() else {}
    current.update(updates)
    return json.dumps(current, indent=2) + "\n"


def planned_item_files(root: Path, payload: dict, content_id: int) -> list[tuple[Path, str]]:
    label = payload["label"]
    gameplay_path = items_dir(root) / f"{label}.json"
    presentation_path = presentation_dir(root) / f"{label}.json"
    planned = [
        (
            gameplay_path,
            _merged(
                gameplay_path,
                {
                    "schema_version": SCHEMA_VERSION,
                    "id": content_id,
                    "label": label,
                    "category": payload["category"],
                    "stack_limit": int(payload["stack_limit"]),
                    "drop_requires_confirmation": bool(payload.get("drop_requires_confirmation", False)),
                },
            ),
        ),
        (
            presentation_path,
            _merged(
                presentation_path,
                {
                    "schema_version": PRESENTATION_SCHEMA,
                    "id": content_id,
                    "label": label,
                    "icon": payload["icon"],
                    "display_name": payload.get("display_name", ""),
                    "description": payload.get("description", ""),
                },
            ),
        ),
    ]
    if payload["category"] == "equipment":
        equip = equipment_path(root, content_id) or equipment_dir(root) / f"{label}.json"
        planned.append(
            (
                equip,
                _merged(
                    equip,
                    {
                        "schema_version": EQUIPMENT_SCHEMA,
                        "id": content_id,
                        "label": label,
                        "equipment_slot": payload["equipment_slot"],
                    },
                ),
            )
        )
    notes_path = notes_dir(root) / f"{content_id}.json"
    planned.append(
        (
            notes_path,
            json.dumps(
                {"notes": payload.get("notes", ""), "tags": payload.get("tags") or []},
                indent=2,
            )
            + "\n",
        )
    )
    return planned


def publish_files(root: Path, files: list[tuple[Path, str | bytes]], *, validate: bool) -> AuthoringOperation:
    operation = AuthoringOperation(root)
    for path, payload in files:
        body = payload.encode("utf-8") if isinstance(payload, str) else payload
        operation.stage(path, body)
    try:
        operation.publish()
        if validate:
            validate_pack(root)
        operation.finish()
    except Exception:
        operation.rollback()
        raise
    return operation


def _same_draft(current: dict, payload: dict) -> bool:
    return (
        current.get("category") == payload.get("category")
        and int(current.get("stack_limit", 0)) == int(payload.get("stack_limit", 0))
        and (current.get("display_name") or "") == (payload.get("display_name") or "")
        and (current.get("description") or "") == (payload.get("description") or "")
        and current.get("icon") == payload.get("icon")
        and (current.get("notes") or "") == (payload.get("notes") or "")
        and list(current.get("tags") or []) == list(payload.get("tags") or [])
        and bool(current.get("drop_requires_confirmation"))
        == bool(payload.get("drop_requires_confirmation", False))
    )


class AuthoringRejected(Exception):
    def __init__(self, status: int, payload: dict) -> None:
        super().__init__(payload.get("error") or "; ".join(payload.get("errors") or []))
        self.status = status
        self.payload = payload


def commit_item_create(root: Path, payload: dict) -> dict:
    errors = validate_payload(payload, creating=True)
    if errors:
        raise AuthoringRejected(400, {"errors": errors})
    label = payload["label"]
    with CatalogWriteLock(root):
        recover_authoring(root)
        existing = items_dir(root) / f"{label}.json"
        if existing.is_file():
            current = load_item(root, existing)
            if _same_draft(current, payload):
                return {"content_id": current["content_id"], "item": current, "idempotent": True}
            raise AuthoringConflict(
                f"{label} already exists. Reload it instead of allocating another id."
            )
        catalog_path = catalog_rs(root)
        ledger_path = catalog_md(root)
        lib_path = common_lib(root)
        catalog = catalog_path.read_text(encoding="utf-8").replace("\r\n", "\n").replace("\r", "\n")
        ledger = ledger_path.read_text(encoding="utf-8").replace("\r\n", "\n").replace("\r", "\n")
        library = lib_path.read_text(encoding="utf-8").replace("\r\n", "\n").replace("\r", "\n")
        content_id = next_item_id(catalog, ledger)
        operation = AuthoringOperation(root)
        for path, text in [
            (catalog_path, insert_catalog(catalog, label, content_id)),
            (lib_path, insert_lib_export(library, label)),
            (ledger_path, insert_ledger(ledger, label, content_id)),
            *planned_item_files(root, payload, content_id),
        ]:
            operation.stage(path, text.encode("utf-8"))
        try:
            operation.publish()
            formatted = subprocess.run(
                ["cargo", "fmt", "-p", "purgatory-common"],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            if formatted.returncode != 0:
                raise RuntimeError(formatted.stderr or "cargo fmt failed")
            validate_pack(root)
            operation.finish()
        except Exception:
            operation.rollback()
            raise
    return {"content_id": content_id, "item": load_item(root, items_dir(root) / f"{label}.json")}


def commit_item_save(root: Path, payload: dict) -> dict:
    errors = validate_payload(payload, creating=False)
    if errors:
        raise AuthoringRejected(400, {"errors": errors})
    label = payload["label"]
    path = items_dir(root) / f"{label}.json"
    with CatalogWriteLock(root):
        recover_authoring(root)
        if not path.is_file():
            raise AuthoringRejected(404, {"error": "item was not found"})
        current = load_item(root, path)
        if payload.get("revision") != current["revision"]:
            raise AuthoringConflict("The item changed on disk. Reload before saving.")
        if payload.get("category") != current["category"]:
            raise AuthoringRejected(
                400, {"error": "Changing category is rejected. Duplicate the item instead."}
            )
        if int(payload["stack_limit"]) < int(current["stack_limit"]):
            raise AuthoringRejected(400, {"error": "Reducing stack_limit is rejected."})
        if current["category"] == "equipment" and payload.get("equipment_slot") != current["equipment_slot"]:
            raise AuthoringRejected(400, {"error": "Changing the equipment slot is rejected."})
        publish_files(root, planned_item_files(root, payload, int(current["content_id"])), validate=True)
        saved = load_item(root, path)
    return {"item": saved}


def commit_icon(root: Path, key: str, data: bytes) -> dict:
    if not VISUAL_RE.fullmatch(key):
        raise AuthoringRejected(400, {"error": "icon key is invalid"})
    try:
        width, height, color = png_size(data)
    except (ValueError, IndexError) as error:
        raise AuthoringRejected(400, {"error": str(error)}) from error
    if (width, height) != (ICON_PX, ICON_PX) or color != 6:
        raise AuthoringRejected(400, {"error": "icon must be a 32x32 RGBA PNG"})
    destination = icons_dir(root) / f"{key}.png"
    with CatalogWriteLock(root):
        recover_authoring(root)
        if destination.exists():
            raise AuthoringRejected(400, {"error": "an icon with that key already exists"})
        publish_files(root, [(destination, data)], validate=False)
    return {"icon": key}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:
        return

    def send_json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def read_body(self) -> dict:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        return json.loads(raw.decode("utf-8"))

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        root = repo_root()
        if parsed.path == "/api/health":
            self.send_json(
                200,
                {
                    "tool": "item-lab",
                    "build": BUILD,
                    "port": PORT,
                    "workspace": str(root),
                },
            )
            return
        if parsed.path == "/api/icons":
            keys = sorted(
                {
                    row.get("icon", "")
                    for row in list_items(root)
                    if row.get("icon")
                }
                | {path.stem for path in icons_dir(root).glob("*.png")}
            )
            self.send_json(200, {"icons": [key for key in keys if key]})
            return
        if parsed.path == "/api/items":
            self.send_json(200, {"items": list_items(root)})
            return
        if parsed.path == "/api/item":
            label = parse_qs(parsed.query).get("label", [""])[0]
            path = items_dir(root) / f"{label}.json"
            if not path.is_file():
                self.send_json(404, {"error": "item was not found"})
                return
            self.send_json(200, load_item(root, path))
            return
        if parsed.path == "/api/where-used":
            content_id = int(parse_qs(parsed.query).get("id", ["0"])[0])
            label = parse_qs(parsed.query).get("label", [""])[0]
            self.send_json(200, where_used(root, content_id, label))
            return
        if parsed.path == "/api/icon":
            key = parse_qs(parsed.query).get("key", [""])[0]
            icon = icons_dir(root) / f"{key}.png"
            if not VISUAL_RE.fullmatch(key) or not icon.is_file():
                self.send_json(404, {"error": "icon was not found"})
                return
            body = icon.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "image/png")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if parsed.path in ("/", "/index.html"):
            self._file(root / "tools" / "item_lab" / "web" / "index.html", "text/html")
            return
        if parsed.path == "/authoring_chart.js":
            self._file(root / "tools" / "authoring_chart.js", "text/javascript")
            return
        if parsed.path.startswith("/") and parsed.path.count("/") == 1:
            candidate = root / "tools" / "item_lab" / "web" / parsed.path.lstrip("/")
            if candidate.is_file():
                kind = "text/css" if candidate.suffix == ".css" else "text/javascript"
                self._file(candidate, kind)
                return
        self.send_json(404, {"error": "not found"})

    def _file(self, path: Path, content_type: str) -> None:
        body = path.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:
        root = repo_root()
        try:
            if self.path == "/api/items":
                self.create_item(root)
            elif self.path == "/api/items/save":
                self.save_item(root)
            elif self.path == "/api/icons":
                self.import_icon(root)
            else:
                self.send_json(404, {"error": "not found"})
        except AuthoringRejected as error:
            self.send_json(error.status, error.payload)
        except AuthoringConflict as error:
            self.send_json(409, {"error": str(error), "conflict": True})
        except TimeoutError as error:
            self.send_json(503, {"error": str(error)})
        except AuthoringRepair as error:
            self.send_json(409, {"error": str(error), "repair": True})
        except Exception as error:  # surface authoring failures to the page
            self.send_json(400, {"error": str(error)})

    def create_item(self, root: Path) -> None:
        self.send_json(200, commit_item_create(root, self.read_body()))

    def save_item(self, root: Path) -> None:
        self.send_json(200, commit_item_save(root, self.read_body()))

    def import_icon(self, root: Path) -> None:
        import base64

        payload = self.read_body()
        data = base64.b64decode(payload.get("png_base64", ""))
        self.send_json(200, commit_icon(root, str(payload.get("icon", "")), data))


def main() -> None:
    import argparse
    import threading
    import webbrowser

    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=PORT)
    parser.add_argument("--open", action="store_true")
    args = parser.parse_args()
    try:
        notes = recover_authoring(repo_root())
    except AuthoringRepair as error:
        raise SystemExit(str(error)) from error
    for note in notes:
        print(f"ITEM_LAB|RECOVERY|{note}", flush=True)
    server = ThreadingHTTPServer((HOST, args.port), Handler)
    print(f"Item Lab http://{HOST}:{args.port} build {BUILD}", flush=True)
    if args.open:
        threading.Timer(0.3, lambda: webbrowser.open(f"http://{HOST}:{args.port}/")).start()
    server.serve_forever()


if __name__ == "__main__":
    main()
