#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
python3 -m unittest discover -s ./tools/mob_lab -p "test_*.py"
cargo run -p purgatory-content-validator -q

echo "PURGATORY quality gate OK"
