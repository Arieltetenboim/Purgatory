#!/usr/bin/env bash
# Extended PURGATORY network soak (Phase 5.0F).
#
# Runs the `#[ignore]` network soak tests only. Longer than scripts/check.sh,
# which stays fast for routine use. Localhost only; no internet, no external
# services, no project state changes. A pass proves convergence and
# boundedness, NOT player capacity.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for package in purgatory-server purgatory-client; do
  echo ">> cargo test -p ${package} -- --ignored --test-threads=1"
  cargo test -p "${package}" -- --ignored --test-threads=1
done

echo "PURGATORY extended network soak OK"
