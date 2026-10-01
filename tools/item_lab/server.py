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
    gameplay_text = path.read_text(encoding="utf-8")
    presentation_text = presentation_path.read_text(encoding="utf-8") if presentation_path.exists() else ""
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
        "revision": revision_of(gameplay_text, presentation_text),
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


def write_item_files(root: Path, payload: dict, content_id: int) -> None:
    label = payload["label"]
    gameplay = {
        "schema_version": SCHEMA_VERSION,
        "id": content_id,
        "label": label,
        "category": payload["category"],
        "stack_limit": int(payload["stack_limit"]),
        "drop_requires_confirmation": bool(payload.get("drop_requires_confirmation", False)),
    }
    presentation = {
        "schema_version": PRESENTATION_SCHEMA,
        "id": content_id,
        "label": label,
        "icon": payload["icon"],
        "display_name": payload.get("display_name", ""),
        "description": payload.get("description", ""),
    }
    atomic_write(items_dir(root) / f"{label}.json", json.dumps(gameplay, indent=2) + "\n")
    atomic_write(
        presentation_dir(root) / f"{label}.json",
        json.dumps(presentation, indent=2) + "\n",
    )
    if payload["category"] == "equipment":
        equipment = {
            "schema_version": EQUIPMENT_SCHEMA,
            "id": content_id,
            "label": label,
            "equipment_slot": payload["equipment_slot"],
        }
        atomic_write(
            equipment_dir(root) / f"{label}.json",
            json.dumps(equipment, indent=2) + "\n",
        )
    notes = {"notes": payload.get("notes", ""), "tags": payload.get("tags") or []}
    atomic_write(notes_dir(root) / f"{content_id}.json", json.dumps(notes, indent=2) + "\n")


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
            self.send_json(200, {"tool": "item-lab", "build": BUILD, "port": PORT})
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
        except Exception as error:  # surface authoring failures to the page
            self.send_json(400, {"error": str(error)})

    def create_item(self, root: Path) -> None:
        payload = self.read_body()
        errors = validate_payload(payload, creating=True)
        if errors:
            self.send_json(400, {"errors": errors})
            return
        touched: dict[Path, str | None] = {}
        with CatalogWriteLock(root):
            catalog_path = catalog_rs(root)
            ledger_path = catalog_md(root)
            lib_path = common_lib(root)
            originals = {
                catalog_path: catalog_path.read_text(encoding="utf-8"),
                ledger_path: ledger_path.read_text(encoding="utf-8"),
                lib_path: lib_path.read_text(encoding="utf-8"),
            }
            content_id = next_item_id(originals[catalog_path], originals[ledger_path])
            label = payload["label"]
            created = [
                items_dir(root) / f"{label}.json",
                presentation_dir(root) / f"{label}.json",
                equipment_dir(root) / f"{label}.json",
                notes_dir(root) / f"{content_id}.json",
            ]
            for path in created:
                touched[path] = path.read_text(encoding="utf-8") if path.exists() else None
            try:
                catalog = originals[catalog_path].replace("\r\n", "\n").replace("\r", "\n")
                atomic_write(catalog_path, insert_catalog(catalog, label, content_id))
                atomic_write(lib_path, insert_lib_export(originals[lib_path].replace("\r\n", "\n"), label))
                atomic_write(ledger_path, insert_ledger(originals[ledger_path].replace("\r\n", "\n"), label, content_id))
                write_item_files(root, payload, content_id)
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
            except Exception:
                for path, previous in originals.items():
                    atomic_write(path, previous)
                for path, previous in touched.items():
                    if previous is None and path.exists():
                        path.unlink()
                    elif previous is not None:
                        atomic_write(path, previous)
                raise
        self.send_json(200, {"content_id": content_id, "item": load_item(root, items_dir(root) / f"{label}.json")})

    def save_item(self, root: Path) -> None:
        payload = self.read_body()
        errors = validate_payload(payload, creating=False)
        if errors:
            self.send_json(400, {"errors": errors})
            return
        label = payload["label"]
        path = items_dir(root) / f"{label}.json"
        if not path.is_file():
            self.send_json(404, {"error": "item was not found"})
            return
        current = load_item(root, path)
        if payload.get("revision") != current["revision"]:
            self.send_json(409, {"error": "The item changed on disk. Reload before saving."})
            return
        if payload.get("category") != current["category"]:
            self.send_json(400, {"error": "Changing category is rejected. Duplicate the item instead."})
            return
        if int(payload["stack_limit"]) < int(current["stack_limit"]):
            self.send_json(400, {"error": "Reducing stack_limit is rejected."})
            return
        if current["category"] == "equipment" and payload.get("equipment_slot") != current["equipment_slot"]:
            self.send_json(400, {"error": "Changing the equipment slot is rejected."})
            return
        content_id = int(current["content_id"])
        presentation_path = presentation_dir(root) / f"{label}.json"
        notes_path = notes_dir(root) / f"{content_id}.json"
        previous = {
            path: path.read_text(encoding="utf-8"),
            presentation_path: presentation_path.read_text(encoding="utf-8"),
            notes_path: notes_path.read_text(encoding="utf-8") if notes_path.exists() else None,
        }
        try:
            write_item_files(root, payload, content_id)
            validate_pack(root)
        except Exception:
            for file_path, text in previous.items():
                if text is None and file_path.exists():
                    file_path.unlink()
                elif text is not None:
                    atomic_write(file_path, text)
            raise
        self.send_json(200, {"item": load_item(root, path)})

    def import_icon(self, root: Path) -> None:
        payload = self.read_body()
        key = str(payload.get("icon", ""))
        if not VISUAL_RE.fullmatch(key):
            self.send_json(400, {"error": "icon key is invalid"})
            return
        raw = payload.get("png_base64", "")
        import base64

        data = base64.b64decode(raw)
        width, height, color = png_size(data)
        if (width, height) != (ICON_PX, ICON_PX) or color != 6:
            self.send_json(400, {"error": "icon must be a 32x32 RGBA PNG"})
            return
        destination = icons_dir(root) / f"{key}.png"
        if destination.exists():
            self.send_json(400, {"error": "an icon with that key already exists"})
            return
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        self.send_json(200, {"icon": key})


def main() -> None:
    import argparse
    import threading
    import webbrowser

    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=PORT)
    parser.add_argument("--open", action="store_true")
    args = parser.parse_args()
    server = ThreadingHTTPServer((HOST, args.port), Handler)
    print(f"Item Lab http://{HOST}:{args.port} build {BUILD}", flush=True)
    if args.open:
        threading.Timer(0.3, lambda: webbrowser.open(f"http://{HOST}:{args.port}/")).start()
    server.serve_forever()


if __name__ == "__main__":
    main()
