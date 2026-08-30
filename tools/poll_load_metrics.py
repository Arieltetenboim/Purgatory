#!/usr/bin/env python3
"""Poll LoadMetricsV1 (schema 2) at 1 Hz. Sidecar for Phase 6D characterization.

Does not go through the harness CSV. Missed polls are omitted (not zero-filled).
"""

from __future__ import annotations

import argparse
import json
import socket
import sys
import time
from pathlib import Path

MAGIC = b"PURGSTAT"
VERSION = 1
REQUEST = MAGIC + bytes([VERSION])


def decode_response(data: bytes) -> dict | None:
    if len(data) < 11:
        return None
    if data[:8] != MAGIC or data[8] != VERSION:
        return None
    length = int.from_bytes(data[9:11], "little")
    if len(data) != 11 + length:
        return None
    try:
        return json.loads(data[11:])
    except json.JSONDecodeError:
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=5002)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--out", required=True, help="JSONL output path")
    parser.add_argument(
        "--until-file",
        help="Stop when this file exists (written by the runner after the harness exits)",
    )
    parser.add_argument("--max-secs", type=float, default=600.0)
    args = parser.parse_args()

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    stop = Path(args.until_file) if args.until_file else None
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(0.4)
    t0 = time.monotonic()
    with out.open("w", encoding="utf-8") as fh:
        while True:
            elapsed = time.monotonic() - t0
            if elapsed >= args.max_secs:
                break
            if stop is not None and stop.is_file():
                break
            try:
                sock.sendto(REQUEST, (args.host, args.port))
                data, _ = sock.recvfrom(2048)
                metrics = decode_response(data)
            except OSError:
                metrics = None
            if metrics is not None:
                rec = {
                    "wall_elapsed_s": round(elapsed, 3),
                    "unix_s": time.time(),
                    "metrics": metrics,
                }
                fh.write(json.dumps(rec, separators=(",", ":")) + "\n")
                fh.flush()
            time.sleep(args.interval)
    sock.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
