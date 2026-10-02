# Item Lab V1 verification

Branch `forge/item-lab-v1` in `C:\Users\Ariel\OneDrive\Desktop\Purgatory\item-lab-v1`. Reviewed head before these corrections: `aa7104ee0ebdd7d4c2544088a2b630811036b4d3`. Reviewed base: `origin/master` `5397a71f689cf541a69fd3ec28fda8d4f099d3ad`. `origin/master` had not moved at fetch time. The checkout at `Purgatory\1` was not modified.

Saving rules: [`PERSISTENCE_AND_AUTHORING_CONTRACT.md`](PERSISTENCE_AND_AUTHORING_CONTRACT.md).

## Automated

Commands run from this worktree on Windows, Python 3.13, stable Rust:

- `cargo test -p purgatory-server production_rng -- --test-threads=8` — passed. 10,000 deaths of a 100% quantity 1–2 row hit both endpoints (each more than 4,000) and the mean stayed within 0.05 of 1.5. The same production `LootRng` plus `roll_monster_drops` covered a fixed quantity, a 1–6 range, independent 0% and 100% rows, and a 10% row (successes in 700..1300 of 10,000, not an exact quota).
- `cargo test -p purgatory-server postgres_monster_death_pickup_survives_restart -- --ignored` — **NOT RUN**. `PURGATORY_TEST_DATABASE_URL` is unset. The harness panicked before opening a database: `PURGATORY_TEST_DATABASE_URL is unset, so this PostgreSQL test was not executed`. This is not a passed death → reserve → pickup → restore proof. Memory tests that install ids with `install_reserved_ids_for_test` are not that proof either.
- `./scripts/check.ps1` — passed (fmt, check, clippy `-D warnings`, workspace tests, Mob Lab unittest 39 tests, content validator OK: 4 monsters, 8 equipment). The default workspace test run skips `#[ignore]` PostgreSQL tests.
- `py -3 -m unittest discover -s .\tools\mob_lab -p test_*.py` — passed, included in the gate. `test_authoring_save.py` exercises the production save functions and `tools/authoring_save.py`: one success and one HTTP-level conflict from the same revision, notes-only invalidation, equipment rollback, injected failure at each publication index, a child process that exits after publish and before `finish`, external-byte fail-closed recovery, create retry without a second id, stack reduction and a non-PNG icon, lock timeout then a later acquire, retired id skip, preserved extra JSON, and distinct item/monster catalog ids.
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
