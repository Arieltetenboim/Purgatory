"""Small repository-backed catalogs for NPC Lab authored references.

The HTTP/API shape is intentionally generic so later reference pickers (items,
abilities, etc.) can reuse it instead of adding one endpoint per content type.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Literal


IdMode = Literal["stem", "json_id"]


@dataclass(frozen=True)
class CatalogSource:
    relative_root: str
    suffix: str
    id_mode: IdMode


CATALOG_SOURCES: dict[str, CatalogSource] = {
    "animation": CatalogSource(
        relative_root="content/shared/animations",
        suffix=".anim",
        id_mode="stem",
    ),
    # Future item picker: add an `item` source rooted at content/shared/items
    # with id_mode="json_id". The endpoint and UI contract do not need to change.
}


def _entry_id(path: Path, source: CatalogSource) -> str:
    if source.id_mode == "stem":
        return path.stem
    if source.id_mode == "json_id":
        with path.open("r", encoding="utf-8") as fh:
            value = json.load(fh)
        authored_id = value.get("id") if isinstance(value, dict) else None
        if not isinstance(authored_id, str) or not authored_id.strip():
            raise ValueError(f"Catalog file has no non-empty id: {path}")
        return authored_id.strip()
    raise ValueError(f"Unsupported catalog id mode: {source.id_mode}")


def list_catalog(repo_root: Path, kind: str) -> list[dict[str, str]]:
    source = CATALOG_SOURCES.get(kind)
    if source is None:
        raise ValueError(f"Unsupported catalog kind: {kind!r}.")

    root = repo_root / source.relative_root
    if not root.is_dir():
        return []

    entries: list[dict[str, str]] = []
    by_id: dict[str, str] = {}
    for path in sorted(root.rglob(f"*{source.suffix}")):
        if not path.is_file():
            continue
        authored_id = _entry_id(path, source)
        relative = path.relative_to(repo_root).as_posix()
        previous = by_id.get(authored_id)
        if previous is not None:
            raise ValueError(
                f"Catalog kind {kind!r} has duplicate id {authored_id!r}: "
                f"{previous} and {relative}."
            )
        by_id[authored_id] = relative
        entries.append({"id": authored_id, "path": relative})

    entries.sort(key=lambda entry: (entry["id"].lower(), entry["path"]))
    return entries
