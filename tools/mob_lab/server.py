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
SCHEMA_VERSION = 4
ID_RE = re.compile(r"^monster\.[a-z0-9][a-z0-9._-]*$")
SPRITE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")


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

    sprite = value.get("sprite")
    if not isinstance(sprite, str) or not SPRITE_ID_RE.fullmatch(sprite):
        errors.append("sprite must be a valid authored sprite id.")

    for key in ("health_max", "movement_speed"):
        number = value.get(key)
        if not isinstance(number, (int, float)) or isinstance(number, bool) or number <= 0:
            errors.append(f"{key} must be greater than zero.")

    bounds = value.get("collision_bounds")
    if not isinstance(bounds, dict):
        errors.append("collision_bounds must be an object.")
    else:
        for key in ("left", "right", "bottom", "top"):
            number = bounds.get(key)
            if (
                not isinstance(number, (int, float))
                or isinstance(number, bool)
                or number < 0
            ):
                errors.append(f"collision_bounds.{key} must be a non-negative number.")
        if all(isinstance(bounds.get(k), (int, float)) for k in ("left", "right")):
            if bounds["left"] + bounds["right"] <= 0:
                errors.append("collision_bounds horizontal span must be greater than zero.")
        if all(isinstance(bounds.get(k), (int, float)) for k in ("bottom", "top")):
            if bounds["bottom"] + bounds["top"] <= 0:
                errors.append("collision_bounds vertical span must be greater than zero.")

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
        "sprite",
        "health_max",
        "collision_bounds",
        "movement_speed",
        "behavior",
    }
    unknown = sorted(set(value) - allowed)
    if unknown:
        errors.append(f"unknown top-level field(s): {', '.join(unknown)}.")

    return errors


def load_numeric_catalog(repo_root: Path) -> dict[str, int]:
    source = (repo_root / "crates" / "common" / "src" / "content_catalog.rs").read_text(
        encoding="utf-8"
    )
    constants = {
        name: int(raw.replace("_", ""))
        for name, raw in re.findall(
            r"pub const ([A-Z0-9_]+): ContentId = ContentId::from_raw\(([0-9_]+)\);",
            source,
        )
    }
    labels: dict[str, int] = {}
    for label, constant in re.findall(r'"([^"]+)" => ([A-Z0-9_]+),', source):
        raw = constants.get(constant)
        if raw is not None:
            labels[label] = raw
    return labels


def _positive_pair(value: Any) -> bool:
    return (
        isinstance(value, list)
        and len(value) == 2
        and all(
            isinstance(item, (int, float))
            and not isinstance(item, bool)
            and item > 0
            for item in value
        )
    )


def _sprite_record(repo_root: Path, manifest_path: Path) -> dict[str, Any]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("manifest must be an object")
    sprite_id = manifest.get("id")
    if (
        manifest.get("schema_version") != 1
        or manifest.get("kind") != "purgatory_sprite_animation"
        or not isinstance(sprite_id, str)
        or not SPRITE_ID_RE.fullmatch(sprite_id)
    ):
        raise ValueError("unsupported sprite manifest header")

    atlas = manifest.get("atlas")
    if not isinstance(atlas, str) or Path(atlas).name != atlas:
        raise ValueError("atlas must be a local file name")
    atlas_path = manifest_path.parent / atlas
    if not atlas_path.is_file():
        raise ValueError(f"atlas not found: {atlas}")

    frame_size = manifest.get("frame_size_px")
    if (
        not isinstance(frame_size, list)
        or len(frame_size) != 2
        or any(not isinstance(item, int) or item <= 0 for item in frame_size)
    ):
        raise ValueError("frame_size_px must contain two positive integers")

    if not _positive_pair(manifest.get("world_size")):
        raise ValueError("world_size must contain two positive numbers")
    frame_seconds = manifest.get("frame_seconds")
    if (
        not isinstance(frame_seconds, (int, float))
        or isinstance(frame_seconds, bool)
        or frame_seconds <= 0
    ):
        raise ValueError("frame_seconds must be greater than zero")
    if manifest.get("authored_facing") not in ("left", "right"):
        raise ValueError("authored_facing must be left or right")

    clips = manifest.get("clips")
    if not isinstance(clips, dict):
        raise ValueError("clips must be an object")
    for required in ("idle", "move", "attack"):
        clip = clips.get(required)
        frames = clip.get("frames") if isinstance(clip, dict) else None
        if not isinstance(frames, list) or not frames or any(
            not isinstance(frame, int) or frame < 0 for frame in frames
        ):
            raise ValueError(f"{required} clip must contain frame indices")
        if not isinstance(clip.get("loop"), bool):
            raise ValueError(f"{required}.loop must be boolean")

    idle_frames = clips["idle"]["frames"]
    return {
        "id": sprite_id,
        "manifest_path": manifest_path.relative_to(repo_root).as_posix(),
        "atlas": atlas,
        "atlas_path": atlas_path,
        "frame_size_px": frame_size,
        "idle_frames": idle_frames,
        "frame_seconds": float(frame_seconds),
        "world_size": [float(manifest["world_size"][0]), float(manifest["world_size"][1])],
        "authored_facing": manifest["authored_facing"],
    }


def scan_sprite_manifests(repo_root: Path) -> tuple[list[dict[str, Any]], list[str]]:
    root = repo_root / "Graphic" / "creature"
    items: list[dict[str, Any]] = []
    issues: list[str] = []
    if not root.is_dir():
        return items, [f"Sprite root not found: {root}"]
    for manifest_path in sorted(root.glob("*/manifest.json")):
        try:
            items.append(_sprite_record(repo_root, manifest_path))
        except (OSError, ValueError, json.JSONDecodeError) as exc:
            issues.append(f"{manifest_path.relative_to(repo_root).as_posix()}: {exc}")
    items.sort(key=lambda item: item["id"])
    return items, issues


def load_sprite_record(repo_root: Path, sprite_id: str) -> dict[str, Any]:
    items, issues = scan_sprite_manifests(repo_root)
    for item in items:
        if item["id"] == sprite_id:
            return item
    detail = f" ({'; '.join(issues)})" if issues else ""
    raise ValueError(f"Sprite '{sprite_id}' was not found in Graphic/creature{detail}.")


def sprite_public_record(item: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": item["id"],
        "manifest_path": item["manifest_path"],
        "atlas": item["atlas"],
        "frame_size_px": item["frame_size_px"],
        "idle_frames": item["idle_frames"],
        "frame_seconds": item["frame_seconds"],
        "world_size": item["world_size"],
        "authored_facing": item["authored_facing"],
    }


def presentation_payload(item: dict[str, Any]) -> dict[str, Any]:
    sprite_id = item["id"]
    payload = sprite_public_record(item)
    payload.update(
        {
            "available": True,
            "manifest_id": sprite_id,
            "atlas_url": "/api/presentation-atlas?sprite="
            + urllib.parse.quote(sprite_id, safe=""),
        }
    )
    return payload


def new_monster_document(
    authored_id: str,
    debug_name: str,
    sprite_id: str = "creature.red_slime",
) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "id": authored_id,
        "debug_name": debug_name,
        "sprite": sprite_id,
        "health_max": 20.0,
        "collision_bounds": {
            "left": 0.4,
            "right": 0.4,
            "bottom": 0.6,
            "top": 0.6,
        },
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
        if parsed.path == "/api/sprites":
            self._handle_sprites()
            return
        if parsed.path == "/api/presentation":
            self._handle_presentation()
            return
        if parsed.path == "/api/presentation-atlas":
            self._handle_presentation_atlas()
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

    def _handle_sprites(self) -> None:
        items, issues = scan_sprite_manifests(self.repo_root)
        self._json_response(
            {
                "items": [sprite_public_record(item) for item in items],
                "issues": issues,
            }
        )

    def _handle_presentation(self) -> None:
        try:
            sprite_id = self._query_value("sprite")
            self._json_response(presentation_payload(load_sprite_record(self.repo_root, sprite_id)))
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _handle_presentation_atlas(self) -> None:
        try:
            sprite_id = self._query_value("sprite")
            item = load_sprite_record(self.repo_root, sprite_id)
            payload = item["atlas_path"].read_bytes()
            self.send_response(HTTPStatus.OK)
            self.send_header("Content-Type", "image/png")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _handle_list(self) -> None:
        items: list[dict[str, Any]] = []
        catalog = load_numeric_catalog(self.repo_root)
        for path in sorted(self.definitions_root.glob("*.json")):
            entry: dict[str, Any] = {
                "path": path.name,
                "id": None,
                "debug_name": None,
                "sprite": None,
                "schema_version": None,
                "valid": False,
                "error": None,
                "content_id": None,
            }
            try:
                doc = json.loads(path.read_text(encoding="utf-8"))
                errors = validate_monster_document(doc)
                authored = doc.get("id") if isinstance(doc, dict) else None
                entry.update(
                    {
                        "id": authored,
                        "content_id": catalog.get(authored) if isinstance(authored, str) else None,
                        "debug_name": doc.get("debug_name") if isinstance(doc, dict) else None,
                        "sprite": doc.get("sprite") if isinstance(doc, dict) else None,
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
            authored = doc.get("id") if isinstance(doc, dict) else None
            self._json_response(
                {
                    "path": path.name,
                    "document": doc,
                    "content_id": load_numeric_catalog(self.repo_root).get(authored)
                    if isinstance(authored, str)
                    else None,
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
        try:
            load_sprite_record(self.repo_root, doc["sprite"])
        except (ValueError, OSError, json.JSONDecodeError) as exc:
            return False, [str(exc)], ""

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
            catalog = load_numeric_catalog(self.repo_root)
            if authored_id not in catalog:
                self._json_response(
                    {
                        "error": (
                            f"{authored_id} has no stable numeric ContentId allocation. "
                            "Allocate it through the checked content catalog first "
                            "(tracked by issue #24), then create the Monster definition."
                        )
                    },
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                )
                return
            path = resolve_monster_path(self.definitions_root, safe_filename(authored_id))
            if path.exists():
                self._json_response(
                    {"error": f"Monster already exists at {path.name}."},
                    HTTPStatus.CONFLICT,
                )
                return
            sprites, sprite_issues = scan_sprite_manifests(self.repo_root)
            if not sprites:
                detail = "; ".join(sprite_issues) if sprite_issues else "no manifests found"
                raise ValueError(f"Cannot create Monster without a valid sprite: {detail}")
            doc = new_monster_document(authored_id, debug_name, sprites[0]["id"])
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
    handler = lambda *hargs, **hkwargs: MobLabHandler(
        *hargs, directory=str(web_root), **hkwargs
    )
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
