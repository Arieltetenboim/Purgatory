"""One file-journal save for Item Lab and Mob Lab.

Callers hold ``CatalogWriteLock`` for the whole read/check/publish. This helper
does not take that lock, so it must not be nested inside another acquisition
of the same lock. PostgreSQL gameplay persistence is a different owner.
"""

from __future__ import annotations

import hashlib
import json
import os
import uuid
from pathlib import Path


class AuthoringConflict(Exception):
    """The draft revision is no longer the saved resource set."""


class AuthoringRepair(Exception):
    """A crashed publish left bytes this recovery will not guess over."""


def source_revision(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def recovery_root(repo_root: Path) -> Path:
    return repo_root / "content" / ".authoring-recovery"


def _rel(repo_root: Path, path: Path) -> str:
    return path.resolve().relative_to(repo_root.resolve()).as_posix()


def _abs(repo_root: Path, relative: str) -> Path:
    return repo_root / relative


def _read(path: Path) -> bytes | None:
    if not path.exists():
        return None
    return path.read_bytes()


def _write(path: Path, payload: bytes | None) -> None:
    if payload is None:
        if path.exists():
            path.unlink()
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + f".{uuid.uuid4().hex}.tmp")
    temporary.write_bytes(payload)
    os.replace(temporary, path)


class AuthoringOperation:
    def __init__(self, repo_root: Path, operation_id: str | None = None) -> None:
        self.repo_root = repo_root
        self.operation_id = operation_id or uuid.uuid4().hex
        self.directory = recovery_root(repo_root) / self.operation_id
        self._staged: list[tuple[str, bytes | None, bytes | None]] = []
        self._opened = False

    def stage(self, path: Path, new_payload: bytes | None) -> None:
        relative = _rel(self.repo_root, path)
        before = _read(path)
        self._staged.append((relative, before, new_payload))

    def publish(self, fail_after: int | None = None) -> None:
        """Replace every staged file. ``fail_after`` is a test fault point.

        Writing the files is not the commit. :meth:`finish` sets status
        ``complete`` only after the caller has validated. A crash while status
        is ``publishing`` is recovered by :func:`recover_authoring`, which
        restores every before-image. Sequential renames are not an
        all-or-nothing multi-file commit. This journal is not gameplay
        persistence and does not fsync; a process crash is the tested boundary.
        """
        if not self._staged:
            return
        self.directory.mkdir(parents=True, exist_ok=False)
        self._opened = True
        manifest = {
            "status": "publishing",
            "operation_id": self.operation_id,
            "files": [],
        }
        for index, (relative, before, after) in enumerate(self._staged):
            before_name = f"{index}.before"
            after_name = f"{index}.after"
            if before is not None:
                (self.directory / before_name).write_bytes(before)
            if after is not None:
                (self.directory / after_name).write_bytes(after)
            manifest["files"].append(
                {
                    "path": relative,
                    "before": before_name if before is not None else None,
                    "after": after_name if after is not None else None,
                }
            )
        _write_json(self.directory / "manifest.json", manifest)
        for index, (relative, _before, after) in enumerate(self._staged):
            if fail_after is not None and index >= fail_after:
                raise RuntimeError(f"injected failure after {fail_after} file(s)")
            _write(_abs(self.repo_root, relative), after)

    def finish(self) -> None:
        if not self._opened:
            return
        manifest = json.loads((self.directory / "manifest.json").read_text(encoding="utf-8"))
        manifest["status"] = "complete"
        _write_json(self.directory / "manifest.json", manifest)
        _remove_tree(self.directory)

    def rollback(self) -> None:
        if not self._opened:
            return
        recover_authoring(self.repo_root)


def recover_authoring(repo_root: Path) -> list[str]:
    """Finish or undo journals left by a crashed Lab process.

    Returns human-readable actions. Raises :class:`AuthoringRepair` when a
    file matches neither its before-image nor its after-image.
    """
    root = recovery_root(repo_root)
    if not root.is_dir():
        return []
    notes: list[str] = []
    for directory in sorted(path for path in root.iterdir() if path.is_dir()):
        manifest_path = directory / "manifest.json"
        if not manifest_path.is_file():
            _remove_tree(directory)
            notes.append(f"removed incomplete journal {directory.name}")
            continue
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        status = manifest.get("status")
        if status == "complete":
            _remove_tree(directory)
            continue
        if status != "publishing":
            _remove_tree(directory)
            notes.append(f"discarded uncommitted journal {directory.name}")
            continue
        files = manifest.get("files") or []
        observed = []
        for entry in files:
            current = _read(_abs(repo_root, entry["path"]))
            before = _named(directory, entry.get("before"))
            after = _named(directory, entry.get("after"))
            if current == after:
                observed.append("after")
            elif current == before:
                observed.append("before")
            else:
                raise AuthoringRepair(
                    f"authoring recovery refused {entry['path']}: its bytes match "
                    f"neither the saved original nor the unpublished edit in {directory.name}"
                )
        for entry in files:
            _write(_abs(repo_root, entry["path"]), _named(directory, entry.get("before")))
        notes.append(f"restored journal {directory.name}")
        _remove_tree(directory)
    if root.is_dir() and not any(root.iterdir()):
        root.rmdir()
    return notes


def _named(directory: Path, name: str | None) -> bytes | None:
    if name is None:
        return None
    return (directory / name).read_bytes()


def _write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8", newline="\n")
    os.replace(temporary, path)


def _remove_tree(path: Path) -> None:
    if not path.exists():
        return
    for child in sorted(path.rglob("*"), reverse=True):
        if child.is_file():
            child.unlink()
        elif child.is_dir():
            child.rmdir()
    path.rmdir()
