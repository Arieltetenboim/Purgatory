"""Cross-process lock for catalog and ledger writes shared by local Labs."""

from __future__ import annotations

import time
from pathlib import Path


class CatalogWriteLock:
    def __init__(self, repo_root: Path, timeout_s: float = 120.0) -> None:
        self.path = repo_root / "content" / ".authoring-catalog.lock"
        self.timeout_s = timeout_s
        self._file = None

    def __enter__(self) -> "CatalogWriteLock":
        import msvcrt

        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._file = self.path.open("a+b")
        deadline = time.monotonic() + self.timeout_s
        while True:
            try:
                self._file.seek(0)
                msvcrt.locking(self._file.fileno(), msvcrt.LK_NBLCK, 1)
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
        import msvcrt

        if self._file is not None:
            try:
                self._file.seek(0)
                msvcrt.locking(self._file.fileno(), msvcrt.LK_UNLCK, 1)
            finally:
                self._file.close()
                self._file = None
