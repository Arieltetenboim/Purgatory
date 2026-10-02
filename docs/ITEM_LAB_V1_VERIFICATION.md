# Item Lab V1 verification

Branch `forge/item-lab-v1` in `C:\Users\Ariel\OneDrive\Desktop\Purgatory\item-lab-v1`. Reviewed head before these corrections: `aa7104ee0ebdd7d4c2544088a2b630811036b4d3`. Reviewed base: `origin/master` `5397a71f689cf541a69fd3ec28fda8d4f099d3ad`. `origin/master` had not moved at fetch time. The checkout at `Purgatory\1` was not modified.

Saving rules: [`PERSISTENCE_AND_AUTHORING_CONTRACT.md`](PERSISTENCE_AND_AUTHORING_CONTRACT.md).

## Automated

Commands run from this worktree on Windows, Python 3.13, stable Rust:

- `cargo test -p purgatory-server production_rng -- --test-threads=8` — passed. 10,000 deaths of a 100% quantity 1–2 row hit both endpoints (each more than 4,000) and the mean stayed within 0.05 of 1.5. The same production `LootRng` plus `roll_monster_drops` covered a fixed quantity, a 1–6 range, independent 0% and 100% rows, and a 10% row (successes in 700..1300 of 10,000, not an exact quota).
- `cargo test -p purgatory-server --bin purgatory-server postgres_12c -- --ignored --test-threads=1` — **20 passed, 0 failed** in 25.00s. This includes `postgres_12c_monster_death_pickup_survives_restart`: a real `reserve_item_ids` for the live channel, a lethal hit, durable pickup, and the same item after restart. Disposable `postgres:18` container, database `purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`. The container was removed after the run. In-memory tests that install ids with `install_reserved_ids_for_test` are not that proof.
- `./scripts/check.ps1` — passed (fmt `--check`, `cargo check --workspace`, clippy `-D warnings`, workspace tests, Mob Lab unittest 44 tests, content validator OK: maps=3, entities=7, monsters=4, equipment=8, defs=44). The default workspace test run skips `#[ignore]` PostgreSQL tests. The verification paragraph was written after that green run and does not change code.
- `py -3 -m unittest discover -s .\tools\mob_lab -p test_authoring_save.py` — 18 passed. The added cases are a validator failure after formatting (live catalog and an unrelated Rust file restored), a reader blocked on a torn save until the writer restores a coherent snapshot, a POSIX `flock` timeout then acquire, an equipment-slot mismatch on create retry, and a PNG whose IHDR is 32×32 but whose image data does not decode.
- `node tools/test_authoring_chart.mjs` — passed, also invoked by that Python test. It calls `tools/authoring_chart.js` for 10% quantity 1, 10% quantity 1–3, 100% quantity 2, 0%, and 0.01% quantity 1–2 (`formatExact` keeps `0.00015`).

In-memory monster loot tests from the earlier commits still run in the workspace suite: lethal character, disconnect before manifestation, partial refill, closed address, empty table. They do not prove a database commit or reconnect restore.

## NOT RUN

Windows graphical session. Remaining checklist:

- Launch the rebuilt Hub from this worktree and open Item Lab.
- Create and save an item with a display name, description, and a supported icon.
- Confirm the canonical files, catalog id, search, and category filter.
- Configure a monster drop in Mob Lab and read both charts, including hover and Inspect N.
- Rebuild for a new catalog id. Restart for a JSON-only edit of an already loaded item.
- Spawn the development monster in a dev-only session without placing it on a production map.
- Kill it, observe the ground icon, pick it up, and read the tooltip quantity.
- Reconnect and confirm the same owned item.

No visual acceptance is claimed.

## Limits

- Authoring recovery is a process-crash journal. It does not fsync and is not a power-loss or database disaster-recovery claim.
- `close_world_address` abandons an unmanifested remainder. Production channel stop uses `lose_all_authority`. The single-address method's callers are tests.
- Sprite-manifest saves and other labs are not on this journal.
- Protocol version stays 34.
