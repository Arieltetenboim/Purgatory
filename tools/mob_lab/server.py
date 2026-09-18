#!/usr/bin/env python3
"""PURGATORY Mob Lab v0.1 local authoring server."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import threading
import urllib.parse
import webbrowser
from http import HTTPStatus
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

HOST = "127.0.0.1"
DEFAULT_PORT = 8766
SCHEMA_VERSION = 2
ID_RE = re.compile(r"^monster\.[a-z0-9][a-z0-9._-]*$")


def validate_monster_document(value: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(value, dict):
        return ["Monster document must be a JSON object."]

    if value.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION}.")

    authored_id = value.get("id")
    if not isinstance(authored_id, str) or not ID_RE.fullmatch(authored_id):
        errors.append("id must use the monster.* authored-id namespace.")

    debug_name = value.get("debug_name")
    if not isinstance(debug_name, str) or not debug_name.strip():
        errors.append("debug_name must be a non-empty string.")

    for key in ("health_max", "movement_speed"):
        number = value.get(key)
        if not isinstance(number, (int, float)) or isinstance(number, bool) or number <= 0:
            errors.append(f"{key} must be greater than zero.")

    half = value.get("half_extents")
    if (
        not isinstance(half, list)
        or len(half) != 2
        or any(
            not isinstance(item, (int, float))
            or isinstance(item, bool)
            or item <= 0
            for item in half
        )
    ):
        errors.append("half_extents must contain two positive numbers.")

    behavior = value.get("behavior")
    if not isinstance(behavior, dict):
        errors.append("behavior must be an object.")
    else:
        if behavior.get("kind") != "chase_contact":
            errors.append("behavior.kind must be chase_contact.")
        if behavior.get("aggro") != "when_attacked":
            errors.append("behavior.aggro must be when_attacked.")
        leash = behavior.get("home_leash_radius")
        if (
            not isinstance(leash, (int, float))
            or isinstance(leash, bool)
            or leash <= 0
        ):
            errors.append("behavior.home_leash_radius must be greater than zero.")

    allowed = {
        "schema_version",
        "id",
        "debug_name",
        "health_max",
        "half_extents",
        "movement_speed",
        "behavior",
    }
    unknown = sorted(set(value) - allowed)
    if unknown:
        errors.append(f"unknown top-level field(s): {', '.join(unknown)}.")

    return errors


def new_monster_document(authored_id: str, debug_name: str) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "id": authored_id,
        "debug_name": debug_name,
        "health_max": 20.0,
        "half_extents": [0.4, 0.6],
        "movement_speed": 2.0,
        "behavior": {
            "kind": "chase_contact",
            "aggro": "when_attacked",
            "home_leash_radius": 3.0,
        },
    }


def resolve_monster_path(root: Path, relative: str) -> Path:
    candidate = (root / relative).resolve()
    resolved_root = root.resolve()
    if candidate.parent != resolved_root or candidate.suffix.lower() != ".json":
        raise ValueError("Monster path must be one JSON file directly under definitions/monsters.")
    return candidate


def safe_filename(authored_id: str) -> str:
    if not ID_RE.fullmatch(authored_id):
        raise ValueError("New monster id must use monster.* and lowercase authored-id characters.")
    return f"{authored_id}.json"


def atomic_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("wb") as fh:
        fh.write(payload)
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(tmp, path)


def validate_runtime_pack(repo_root: Path) -> tuple[bool, str]:
    completed = subprocess.run(
        ["cargo", "run", "-q", "-p", "purgatory-content-validator"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    output = (completed.stdout + completed.stderr).strip()
    return completed.returncode == 0, output


class MobLabHandler(SimpleHTTPRequestHandler):
    repo_root: Path
    definitions_root: Path

    def __init__(self, *args: Any, directory: str | None = None, **kwargs: Any) -> None:
        super().__init__(*args, directory=directory, **kwargs)

    def log_message(self, fmt: str, *args: Any) -> None:
        print(f"MOB_LAB|{self.address_string()}|{fmt % args}")

    def _json_response(self, value: Any, status: HTTPStatus = HTTPStatus.OK) -> None:
        encoded = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _read_json_body(self) -> Any:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        return json.loads(raw.decode("utf-8"))

    def _query_value(self, key: str) -> str:
        parsed = urllib.parse.urlparse(self.path)
        values = urllib.parse.parse_qs(parsed.query).get(key, [])
        if len(values) != 1:
            raise ValueError(f"Expected one '{key}' query parameter.")
        return values[0]

    def do_GET(self) -> None:
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/api/health":
            self._json_response({"ok": True, "tool": "mob-lab", "slice": "M3"})
            return
        if parsed.path == "/api/monsters":
            self._handle_list()
            return
        if parsed.path == "/api/monster":
            self._handle_open()
            return
        if parsed.path == "/":
            self.path = "/index.html"
        super().do_GET()

    def do_POST(self) -> None:
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/api/monster":
            self._handle_save()
            return
        if parsed.path == "/api/new":
            self._handle_new()
            return
        if parsed.path == "/api/validate":
            self._handle_validate()
            return
        self._json_response({"error": "Unknown API route."}, HTTPStatus.NOT_FOUND)

    def _handle_list(self) -> None:
        items: list[dict[str, Any]] = []
        for path in sorted(self.definitions_root.glob("*.json")):
            entry: dict[str, Any] = {
                "path": path.name,
                "id": None,
                "debug_name": None,
                "schema_version": None,
                "valid": False,
                "error": None,
            }
            try:
                doc = json.loads(path.read_text(encoding="utf-8"))
                errors = validate_monster_document(doc)
                entry.update(
                    {
                        "id": doc.get("id") if isinstance(doc, dict) else None,
                        "debug_name": doc.get("debug_name") if isinstance(doc, dict) else None,
                        "schema_version": doc.get("schema_version") if isinstance(doc, dict) else None,
                        "valid": not errors,
                        "error": "; ".join(errors) if errors else None,
                    }
                )
            except (OSError, json.JSONDecodeError) as exc:
                entry["error"] = str(exc)
            items.append(entry)
        self._json_response({"items": items})

    def _handle_open(self) -> None:
        try:
            path = resolve_monster_path(self.definitions_root, self._query_value("path"))
            if not path.is_file():
                self._json_response({"error": "Monster file not found."}, HTTPStatus.NOT_FOUND)
                return
            doc = json.loads(path.read_text(encoding="utf-8"))
            self._json_response(
                {
                    "path": path.name,
                    "document": doc,
                    "validation_errors": validate_monster_document(doc),
                }
            )
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _handle_validate(self) -> None:
        try:
            doc = self._read_json_body()
            errors = validate_monster_document(doc)
            self._json_response({"ok": not errors, "validation_errors": errors})
        except (ValueError, json.JSONDecodeError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _save_candidate(self, path: Path, doc: Any) -> tuple[bool, list[str], str]:
        errors = validate_monster_document(doc)
        if errors:
            return False, errors, ""

        original = path.read_bytes() if path.exists() else None
        encoded = (json.dumps(doc, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        atomic_write(path, encoded)

        ok, output = validate_runtime_pack(self.repo_root)
        if ok:
            return True, [], output

        if original is None:
            path.unlink(missing_ok=True)
        else:
            atomic_write(path, original)
        return False, ["Runtime content validation failed; save rolled back."], output

    def _handle_save(self) -> None:
        try:
            path = resolve_monster_path(self.definitions_root, self._query_value("path"))
            doc = self._read_json_body()
            ok, errors, output = self._save_candidate(path, doc)
            if not ok:
                self._json_response(
                    {
                        "error": "Monster save rejected.",
                        "validation_errors": errors,
                        "validator_output": output,
                    },
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                )
                return
            self._json_response(
                {
                    "ok": True,
                    "path": path.name,
                    "validation_errors": [],
                    "validator_output": output,
                }
            )
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _handle_new(self) -> None:
        try:
            request = self._read_json_body()
            if not isinstance(request, dict):
                raise ValueError("New monster request must be an object.")
            authored_id = str(request.get("id", "")).strip()
            debug_name = str(request.get("debug_name", "")).strip()
            if not debug_name:
                raise ValueError("New monster requires a debug name.")
            path = resolve_monster_path(self.definitions_root, safe_filename(authored_id))
            if path.exists():
                self._json_response({"error": f"Monster already exists at {path.name}."}, HTTPStatus.CONFLICT)
                return
            doc = new_monster_document(authored_id, debug_name)
            ok, errors, output = self._save_candidate(path, doc)
            if not ok:
                self._json_response(
                    {
                        "error": "New monster rejected by runtime validation.",
                        "validation_errors": errors,
                        "validator_output": output,
                    },
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                )
                return
            self._json_response(
                {"ok": True, "path": path.name, "document": doc, "validator_output": output},
                HTTPStatus.CREATED,
            )
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)


def main() -> int:
    parser = argparse.ArgumentParser(description="PURGATORY Mob Lab local server")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--host", default=HOST)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--open", action="store_true")
    args = parser.parse_args()

    repo_root = args.root.resolve()
    web_root = repo_root / "tools" / "mob_lab" / "web"
    definitions_root = repo_root / "content" / "definitions" / "monsters"
    if not web_root.is_dir():
        raise SystemExit(f"Mob Lab web root not found: {web_root}")
    if not definitions_root.is_dir():
        raise SystemExit(f"Monster definitions root not found: {definitions_root}")

    MobLabHandler.repo_root = repo_root
    MobLabHandler.definitions_root = definitions_root
    handler = lambda *hargs, **hkwargs: MobLabHandler(*hargs, directory=str(web_root), **hkwargs)
    server = ThreadingHTTPServer((args.host, args.port), handler)
    url = f"http://{args.host}:{args.port}/"
    print(f"MOB_LAB|READY|{url}")
    if args.open:
        threading.Timer(0.35, lambda: webbrowser.open(url)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
