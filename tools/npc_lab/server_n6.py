#!/usr/bin/env python3
"""PURGATORY NPC Lab N9a local server with presentation-reference catalogs."""

from __future__ import annotations

import argparse
import threading
import urllib.parse
import webbrowser
from http import HTTPStatus
from http.server import ThreadingHTTPServer
from pathlib import Path
from typing import Any

import catalog
import selection
import server

SUPPORTED_POOLS = {"mandatory", "once", "repeatable", "rare", "lore"}
_BASE_VALIDATE_NPC_DOCUMENT = server.validate_npc_document


def validate_npc_document_n7(value: Any) -> list[str]:
    errors = _BASE_VALIDATE_NPC_DOCUMENT(value)
    if not isinstance(value, dict):
        return errors
    interaction = value.get("interaction")
    if not isinstance(interaction, dict):
        return errors
    beats = interaction.get("beats")
    if not isinstance(beats, list):
        return errors

    for index, beat in enumerate(beats):
        if not isinstance(beat, dict):
            continue
        pool = beat.get("pool")
        if pool is None:
            continue
        if not isinstance(pool, str) or pool not in SUPPORTED_POOLS:
            label = beat.get("id") if isinstance(beat.get("id"), str) else f"Beat {index + 1}"
            errors.append(
                f"{label} pool must be one of: {', '.join(sorted(SUPPORTED_POOLS))}."
            )
    return errors


def install_n7_validation() -> None:
    server.validate_npc_document = validate_npc_document_n7


class NpcLabN6Handler(server.NpcLabHandler):
    def do_GET(self) -> None:
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/api/health":
            self._json_response({"ok": True, "tool": "npc-lab", "slice": "N9a-animation-cues"})
            return
        if parsed.path == "/api/catalog":
            self._handle_catalog()
            return
        super().do_GET()

    def do_POST(self) -> None:
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path == "/api/test-preview":
            self._handle_test_preview()
            return
        super().do_POST()

    def _handle_catalog(self) -> None:
        try:
            kind = self._single_query_value("kind")
            items = catalog.list_catalog(self.repo_root, kind)
            self._json_response({"ok": True, "kind": kind, "items": items})
        except (OSError, ValueError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)

    def _handle_test_preview(self) -> None:
        try:
            request = self._read_json_body()
            if not isinstance(request, dict):
                raise ValueError("Test preview request must be an object.")

            document = request.get("document")
            synthetic_state = request.get("state", {})
            operation = request.get("operation", "evaluate")

            errors = server.validate_npc_document(document)
            if errors:
                self._json_response(
                    {
                        "error": "NPC document is not valid enough to preview.",
                        "validation_errors": errors,
                    },
                    HTTPStatus.UNPROCESSABLE_ENTITY,
                )
                return
            if not isinstance(synthetic_state, dict):
                raise ValueError("Synthetic state must be an object.")

            next_state = synthetic_state
            beat: dict[str, Any] | None
            diagnostics: dict[str, Any] | None = None

            if operation == "evaluate":
                beat = selection.select_entry_beat(document, synthetic_state)
                diagnostics = selection.explain_entry_selection(document, synthetic_state)
            elif operation == "advance":
                beat_id = request.get("beat_id")
                choice_id = request.get("choice_id")
                if not isinstance(beat_id, str) or not isinstance(choice_id, str):
                    raise ValueError("Advance requires beat_id and choice_id.")
                next_state, beat = selection.advance_choice(
                    document, synthetic_state, beat_id, choice_id
                )
            elif operation == "complete":
                beat_id = request.get("beat_id")
                if not isinstance(beat_id, str):
                    raise ValueError("Complete requires beat_id.")
                next_state = selection.complete_beat(document, synthetic_state, beat_id)
                beat = None
            else:
                raise ValueError(f"Unsupported preview operation: {operation!r}.")

            self._json_response(
                {
                    "ok": True,
                    "state": next_state,
                    "beat": beat,
                    "ended": beat is None and operation != "evaluate",
                    "diagnostics": diagnostics,
                }
            )
        except (KeyError, TypeError, ValueError) as exc:
            self._json_response({"error": str(exc)}, HTTPStatus.BAD_REQUEST)


def main() -> int:
    parser = argparse.ArgumentParser(description="PURGATORY NPC Lab N9a local server")
    parser.add_argument("--root", type=Path, required=True, help="PURGATORY repository root")
    parser.add_argument("--host", default=server.HOST)
    parser.add_argument("--port", type=int, default=server.DEFAULT_PORT)
    parser.add_argument("--open", action="store_true", help="Open NPC Lab in the default browser")
    args = parser.parse_args()

    install_n7_validation()

    repo_root = args.root.resolve()
    web_root = repo_root / "tools" / "npc_lab" / "web"
    authoring_root = repo_root / "content" / "authoring" / "npcs"
    if not web_root.is_dir():
        raise SystemExit(f"NPC Lab web root not found: {web_root}")
    if not authoring_root.is_dir():
        raise SystemExit(f"NPC authoring root not found: {authoring_root}")

    NpcLabN6Handler.repo_root = repo_root
    NpcLabN6Handler.web_root = web_root
    NpcLabN6Handler.authoring_root = authoring_root

    httpd = ThreadingHTTPServer((args.host, args.port), NpcLabN6Handler)
    url = f"http://{args.host}:{args.port}/"
    print("PURGATORY NPC Lab N9a Animation Cues")
    print(f"Repository: {repo_root}")
    print(f"Authoring:  {authoring_root}")
    print(f"URL:        {url}")
    print("Press Ctrl+C to stop.")

    if args.open:
        threading.Timer(0.35, lambda: webbrowser.open(url)).start()

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping NPC Lab.")
    finally:
        httpd.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
