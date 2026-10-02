# Item Lab V1 verification

Branch `forge/item-lab-v1` in `C:\Users\Ariel\OneDrive\Desktop\Purgatory\item-lab-v1`. Reviewed head before these corrections: `aa7104ee0ebdd7d4c2544088a2b630811036b4d3`. Reviewed base: `origin/master` `5397a71f689cf541a69fd3ec28fda8d4f099d3ad`. `origin/master` had not moved at fetch time. The checkout at `Purgatory\1` was not modified.

Saving rules: [`PERSISTENCE_AND_AUTHORING_CONTRACT.md`](PERSISTENCE_AND_AUTHORING_CONTRACT.md).

## Automated

Commands run from this worktree on Windows, Python 3.13, stable Rust:

- `cargo test -p purgatory-server production_rng -- --test-threads=8` — passed. 10,000 deaths of a 100% quantity 1–2 row hit both endpoints (each more than 4,000) and the mean stayed within 0.05 of 1.5. The same production `LootRng` plus `roll_monster_drops` covered a fixed quantity, a 1–6 range, independent 0% and 100% rows, and a 10% row (successes in 700..1300 of 10,000, not an exact quota).
- `cargo test -p purgatory-server --bin purgatory-server postgres_12c -- --ignored --test-threads=1` — **20 passed, 0 failed** in 10.48s on this commit's tree. This includes `postgres_12c_monster_death_pickup_survives_restart`: a real `reserve_item_ids` for the live channel, a lethal hit, durable pickup, and the same item after restart. Disposable `postgres:18` container, database `purgatory_12a_test` on `127.0.0.1:5433`, not `Purgatory_dev`. The container was removed after the run. In-memory tests that install ids with `install_reserved_ids_for_test` are not that proof.
- `./scripts/check.ps1` — passed (fmt `--check`, `cargo check --workspace`, clippy `-D warnings`, workspace tests, Mob Lab unittest 45 tests, content validator OK: maps=3, entities=7, monsters=4, equipment=8, defs=44). The default workspace test run skips `#[ignore]` PostgreSQL tests. The verification paragraph was written after that green run and does not change code.
- `py -3 -m unittest discover -s .\tools\mob_lab -p test_authoring_save.py` — 19 passed. The added response case holds the first save after commit and before the HTTP body: a second save with the new revision lands, a draft that still names the previous revision returns HTTP 409, and the first response's document and revision still hash to the committed bytes. Earlier cases remain: a validator failure after formatting, a reader blocked on a torn save, a POSIX `flock` timeout then acquire, an equipment-slot mismatch on create retry, and a PNG whose IHDR is 32×32 but whose image data does not decode.
- `node tools/test_authoring_chart.mjs` — passed, also invoked by that Python test. It calls `tools/authoring_chart.js` for 10% quantity 1, 10% quantity 1–3, 100% quantity 2, 0%, and 0.01% quantity 1–2 (`formatExact` keeps `0.00015`).

In-memory monster loot tests from the earlier commits still run in the workspace suite: lethal character, disconnect before manifestation, partial refill, closed address, empty table. They do not prove a database commit or reconnect restore.

## Graphical

Item Lab at `http://127.0.0.1:8767` reported build `item-lab-v1` and this worktree. Mob Lab at `http://127.0.0.1:8766` was the same worktree. `target\debug\purgatory-dev-hub.exe` from this worktree was built and started. The Hub window itself was not clicked.

Passed in the lab pages:

- Created `item.verify.session_scrap` with display name Session scrap, description "A verification scrap from Item Lab.", tag `verify`, and icon key `item.verify.session_scrap`. The 32×32 RGBA PNG was stored through `/api/icons`, the endpoint the file control calls. The file chooser itself was not used. The allocated ContentId was 30000. Opening Sample scrap and then Session scrap again restored the name, description, icon, and tag from the server.
- Search `Session` listed only that item. The equipment category filter hid it.
- Item Lab WHERE USED for Sample scrap (30015) listed `monster.dev.guaranteed_drop`. Inspect N=1000 read expected successes 1000 and expected units 2000. Hover at N 588 read chance 100%, mean quantity 2, expected successes 588, and expected units 1176.
- Mob Lab DROPS for `monster.dev.guaranteed_drop` titled `100% / qty 2–2 / mean 2`. Inspect N=1000 read the same 1000 successes and 2000 units. Hover at N 507 read chance 100%, mean quantity 2, expected successes 507, and expected units 1014. Nothing was saved from that page.

The verification item, its icon, its notes, and the catalog edits were removed after the session. They are not part of the commit. No development monster was written onto a map.

## NOT RUN

- Clicking Item Lab or Mob Lab from the Hub window.
- Rebuilding the client and server for a newly compiled catalog id, restarting them, killing the development monster, reading the ground icon, picking the item up, reading the tooltip quantity, and reconnecting to the same owned item.

Those steps need the native client. This session has no input driver for that window, and the development monster must not be placed on a production map or written through the owner's player database.

## Limits

- Authoring recovery is a process-crash journal. It does not fsync and is not a power-loss or database disaster-recovery claim.
- `close_world_address` abandons an unmanifested remainder. Production channel stop uses `lose_all_authority`. The single-address method's callers are tests.
- Sprite-manifest saves and other labs are not on this journal.
- Protocol version stays 34.
