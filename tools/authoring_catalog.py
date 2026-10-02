"""Cross-process lock for catalog and ledger writes shared by local Labs.

Windows locks one byte with ``msvcrt``. Linux and other POSIX hosts lock the
same file with ``fcntl.flock``. Callers on one machine use one of those
mechanisms. The lock is not re-entrant: do not acquire it again on a thread
that already holds it.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path


def lock_file(file, platform: str) -> None:
    if platform == "win32":
        import msvcrt

        file.seek(0)
        msvcrt.locking(file.fileno(), msvcrt.LK_NBLCK, 1)
        return
    import fcntl

    fcntl.flock(file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)


def unlock_file(file, platform: str) -> None:
    if platform == "win32":
        import msvcrt

        file.seek(0)
        msvcrt.locking(file.fileno(), msvcrt.LK_UNLCK, 1)
        return
    import fcntl

    fcntl.flock(file.fileno(), fcntl.LOCK_UN)


class CatalogWriteLock:
    def __init__(self, repo_root: Path, timeout_s: float = 120.0, platform: str | None = None) -> None:
        self.path = repo_root / "content" / ".authoring-catalog.lock"
        self.timeout_s = timeout_s
        self.platform = platform or sys.platform
        self._file = None

    def __enter__(self) -> "CatalogWriteLock":
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._file = self.path.open("a+b")
        deadline = time.monotonic() + self.timeout_s
        while True:
            try:
                lock_file(self._file, self.platform)
                return self
            except OSError:
                if time.monotonic() >= deadline:
                    self._file.close()
                    self._file = None
                    raise TimeoutError(
                        "Another Lab holds the content catalog lock. Retry after it finishes."
                    )
                time.sleep(0.05)

    def __exit__(self, exc_type, exc, tb) -> None:
        if self._file is not None:
            try:
                unlock_file(self._file, self.platform)
            finally:
                self._file.close()
                self._file = None
