#!/usr/bin/env python3
"""PURGATORY NPC Lab N1 local server.

Stdlib-only local authoring bridge:
- serves tools/npc_lab/web
- reads/writes content/authoring/npcs
- never touches runtime content registries
"""

from __future__ import annotations

import argparse
import json
import os
import threading
import urllib.parse
import webbrowser
from http import HTTPStatus
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


HOST = "127.0.0.1"
DEFAULT_PORT = 8765


def validate_npc_document(value: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(value, dict):
        return ["NPC document must be a JSON object."]

    schema_version = value.get("schema_version")
    if not isinstance(schema_version, int) or schema_version < 1:
        errors.append("schema_version must be an integer >= 1.")

    authored_id = value.get("id")
    if not isinstance(authored_id, str) or not authored_id.strip():
        errors.append("id must be a non-empty string.")
    elif not authored_id.startswith("npc."):
        errors.append("id must use the npc.* authored-id namespace.")

    design = value.get("design")
    if design is not None and not isinstance(design, dict):
        errors.append("design must be an object when present.")

    interaction = value.get("interaction")
    if interaction is not None and not isinstance(interaction, dict):
        errors.append("interaction must be an object when present.")

    return errors


def resolve_npc_path(authoring_root: Path, relative_path: str) -> Path:
    relative_path = relative_path.replace("\\", "/").strip("/")
    if not relative_path or not relative_path.endswith(".json"):
        raise ValueError("NPC path must be a relative .json path.")

    root = authoring_root.resolve()
    candidate = (root / relative_path).resolve()
    try:
        candidate.relative_to(root)
    except ValueError as exc:
        raise ValueError("NPC path escapes the authoring root.") from exc
    return candidate


def authored_area_from_id(authored_id: str) -> str:
    parts = [part for part in authored_id.split(".") if part]
    if len(parts) >= 3 and parts[0] == "npc":
        return parts[1]
    return "unassigned"


def new_npc_document(authored_id: str, area: str) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "id": authored_id,
        "design": {
            "working_name": "",
            "display_name": None,
            "role": "",
            "area": area,
            "tags": [],
            "background": "",
            "personality": [],
            "speech_style": [],
            "gameplay_purposes": [],
            "narrative_purposes": [],
        },
        "relationships": [],
        "interaction": {"beats": []},
        "notes": [],
    }


class NpcLabHandler(SimpleHTTPRequestHandler):
    repo_root: Path
    authoring_root: Path
    web_root: Path

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        super().__init__(*args, directory=str(self.web_root), **kwargs)

    def log_message(self, fmt: str, *args: Any) -> None:
        print(f"[npc-lab] {self.address_string()} - {fmt % args}")

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def _json_response(self, payload: Any, status: int = HTTPStatus.OK) -> None:
        body = json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_json_body(self) -> Any:
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError as exc:
            raise ValueError("Invalid Content-Length.") from exc
        if length <= 0 or length > 4 * 1024 * 1024:
            raise ValueError("Request body is empty or too large.")
        raw = self.rfile.read(length)
        try:
            return json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ValueError(f"Invalid JSON: {exc}") from exc

    def _query(self) -> dict[str, list[str]]:
        parsed = urllib.parse.urlparse(self.path)
        return urllib.parse.parse_qs(parsed.query)

    def _single_query_value(self, key: str) -> str:
        values = self._query().get(key, [])
        if len(values) != 1:
            raise ValueError(f"Expected one '{key}' query parameter.")
        return values[0]

    def do_GET(self) -> None:
        parsed = urllib.parse.urlparse(self.path)

        if parsed.path == "/favicon.ico":
            self.send_response(HTTPStatus.NO_CONTENT)
            self.end_headers()
            return

        if parsed.path == "/api/health":
            self._json_response({"ok": True, "tool": "npc-lab", "slice": "N1"})
            return

        if parsed.path == "/api/npcs":
            self._handle_list_npcs()
            return

        if parsed.path == "/api/npc":
            try:
                relative = self._single_query_value("path")
                path = resolve_npc_path(self.authoring_root, relative)
                if not path.is_file():
                    self._json_response({"error": "NPC file not found."}, HTTPStatus.NOT_FOUND)
                    return
                with path.open("r", encoding="utf-8") as fh:
                    payload = json.load(fh)
                self._json_response(
                    {
                        "path": path.relative_to(self.authoring_root).as_posix(),
                        "document": payload,
                        "validation_errors": validate_npc_document(payload),
                    }
                )
            except (ValueError, OSError, json.JSONDecodeError) as exc:
                self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)
            return

        if parsed.path == "/":
            self.path = "/index.html"

        super().do_GET()

    def do_POST(self) -> None:
        parsed = urllib.parse.urlparse(self.path)

        if parsed.path == "/api/npc":
            self._handle_save_npc()
            return

        if parsed.path == "/api/new":
            self._handle_new_npc()
            return

        self._json_response({"error": "Unknown API route."}, HTTPStatus.NOT_FOUND)

    def _handle_list_npcs(self) -> None:
        items: list[dict[str, Any]] = []
        self.authoring_root.mkdir(parents=True, exist_ok=True)

        for path in sorted(self.authoring_root.rglob("*.json")):
            relative = path.relative_to(self.authoring_root).as_posix()
            entry: dict[str, Any] = {
                "path": relative,
                "id": None,
                "working_name": None,
                "area": None,
                "schema_version": None,
                "valid": False,
                "error": None,
            }
            try:
                with path.open("r", encoding="utf-8") as fh:
                    doc = json.load(fh)
                design = doc.get("design") if isinstance(doc, dict) else None
                if not isinstance(design, dict):
                    design = {}
                errors = validate_npc_document(doc)
                entry.update(
                    {
                        "id": doc.get("id") if isinstance(doc, dict) else None,
                        "working_name": design.get("working_name"),
                        "area": design.get("area"),
                        "schema_version": doc.get("schema_version")
                        if isinstance(doc, dict)
                        else None,
                        "valid": not errors,
                        "error": "; ".join(errors) if errors else None,
                    }
                )
            except (OSError, json.JSONDecodeError) as exc:
                entry["error"] = str(exc)
            items.append(entry)

        self._json_response({"items": items})

    def _handle_save_npc(self) -> None:
        try:
            relative = self._single_query_value("path")
            path = resolve_npc_path(self.authoring_root, relative)
            doc = self._read_json_body()
            errors = validate_npc_document(doc)
            if errors:
                self._json_response(
                    {"error": "NPC document failed N1 validation.", "validation_errors": errors},
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                )
                return

            path.parent.mkdir(parents=True, exist_ok=True)
            tmp = path.with_suffix(path.suffix + ".tmp")
            encoded = (json.dumps(doc, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
            with tmp.open("wb") as fh:
                fh.write(encoded)
                fh.flush()
                os.fsync(fh.fileno())
            os.replace(tmp, path)

            self._json_response(
                {
                    "ok": True,
                    "path": path.relative_to(self.authoring_root).as_posix(),
                    "validation_errors": [],
                }
            )
        except (ValueError, OSError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _handle_new_npc(self) -> None:
        try:
            request = self._read_json_body()
            if not isinstance(request, dict):
                raise ValueError("New NPC request must be an object.")

            authored_id = str(request.get("id", "")).strip()
            if not authored_id.startswith("npc.") or len(authored_id) < 5:
                raise ValueError("New NPC id must be a non-empty npc.* authored id.")

            requested_area = str(request.get("area", "")).strip()
            area = requested_area or authored_area_from_id(authored_id)
            safe_area = "".join(ch for ch in area.lower() if ch.isalnum() or ch in "-_")
            if not safe_area:
                safe_area = "unassigned"

            safe_name = "".join(
                ch for ch in authored_id if ch.isalnum() or ch in "._-"
            )
            relative = f"{safe_area}/{safe_name}.json"
            path = resolve_npc_path(self.authoring_root, relative)
            if path.exists():
                self._json_response(
                    {"error": f"NPC already exists at {relative}."},
                    HTTPStatus.CONFLICT,
                )
                return

            doc = new_npc_document(authored_id, safe_area)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                json.dumps(doc, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
            self._json_response(
                {"ok": True, "path": relative, "document": doc},
                HTTPStatus.CREATED,
            )
        except (ValueError, OSError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)


def main() -> int:
    parser = argparse.ArgumentParser(description="PURGATORY NPC Lab local server")
    parser.add_argument("--root", type=Path, required=True, help="PURGATORY repository root")
    parser.add_argument("--host", default=HOST)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--open", action="store_true", help="Open NPC Lab in the default browser")
    args = parser.parse_args()

    repo_root = args.root.resolve()
    web_root = repo_root / "tools" / "npc_lab" / "web"
    authoring_root = repo_root / "content" / "authoring" / "npcs"

    if not web_root.is_dir():
        raise SystemExit(f"NPC Lab web root not found: {web_root}")
    if not authoring_root.is_dir():
        raise SystemExit(f"NPC authoring root not found: {authoring_root}")

    NpcLabHandler.repo_root = repo_root
    NpcLabHandler.web_root = web_root
    NpcLabHandler.authoring_root = authoring_root

    server = ThreadingHTTPServer((args.host, args.port), NpcLabHandler)
    url = f"http://{args.host}:{args.port}/"
    print(f"PURGATORY NPC Lab N1")
    print(f"Repository: {repo_root}")
    print(f"Authoring:  {authoring_root}")
    print(f"URL:        {url}")
    print("Press Ctrl+C to stop.")

    if args.open:
        threading.Timer(0.35, lambda: webbrowser.open(url)).start()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping NPC Lab.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
