# Item Lab V1 verification

Branch `forge/item-lab-v1` in `C:\Users\Ariel\OneDrive\Desktop\Purgatory\item-lab-v1`. Pull request https://github.com/Arieltetenboim/Purgatory/pull/125 was merged as `c9e37f2`. The V1 matrix below is the record for that branch. The checkout at `Purgatory\1` was not modified.

Saving rules stay in [`PERSISTENCE_AND_AUTHORING_CONTRACT.md`](PERSISTENCE_AND_AUTHORING_CONTRACT.md). This file is the acceptance record. It does not replace that contract.

The existing-equipment save follow-up is recorded below under that heading. Historical matrix results below remain results for their named commits. The later [2026-10-03 closeout audit](ITEM_LAB_CLOSEOUT_2026-10-03.md) records the owner's practice-sword smoke and icon repair separately; it does not turn earlier NOT RUN rows into agent-executed tests. Current full acceptance remains open.

## History, commit `3f2cb13` only

The notes below were true for `3f2cb13b6b77b4547ca3f2d9cf028d9fbfbd4251`. They are not evidence for a later commit.

On that commit, GitHub's quality gate and PostgreSQL job passed. `postgres_12c_monster_death_pickup_survives_restart` ran in that job. Locally, the same ignored filter passed 20 tests in 10.48s on a disposable `postgres:18` database `purgatory_12a_test` at `127.0.0.1:5433`. `./scripts/check.ps1` passed. The authoring file had 19 tests. `node tools/test_authoring_chart.mjs` passed.

Lab pages were opened directly, not from the Hub buttons. Item Lab reported build `item-lab-v1` from this worktree. A temporary item `item.verify.session_scrap` (ContentId 30000) was created through the page, including a 32×32 PNG posted to `/api/icons`. The file chooser was not used. Search, the equipment filter, WHERE USED for Sample scrap, and both expectation graphs were read in the browser, including hover values. That item and its catalog edits were removed and are not in any commit. The Hub window was not clicked. The native client was not used.

## This cycle

Implementation commit: `abc69a9e90a139eef71347abfa673fafbda1549f`.

The commands below ran on that commit's code, from this worktree, before this matrix was added. This file does not change that code. Map JSON line-ending noise in `map.map1.json` and `map.map3.json` was left unstaged.

`rustfmt` is the stable toolchain component (`rustfmt 1.9.0-stable`). `format_rust` writes one independent `source.rs` in a private `purgatory-rustfmt-` directory, passes `--config-path` to the repository `rustfmt.toml` and `--config skip_children=true`, reads the formatted text, and deletes that directory before `AuthoringOperation.stage`. The directory argument is not a write target. No symlink or hard link is used.

### Acceptance matrix

| Requirement | Verification type | Test or evidence | Code version tested | Result | Remaining gap |
|---|---|---|---|---|---|
| Formatting preparation does not write original source files before the journal | Automated, real `rustfmt` | `test_real_rustfmt_does_not_rewrite_a_declared_module`: a root declares `mod sibling`, the sibling is valid Rust the formatter would change, and those bytes stay identical. No `source.rs` remains beside it. | `abc69a9` | PASS | A tree whose modules are already formatted would not show this. This case uses unformatted sibling bytes. |
| Successful creation changes only the intended files | Automated, real `rustfmt` | `test_real_create_formats_only_the_intended_files`: `mod sibling` is planted after the crate docs in the copied `lib.rs`. Sibling bytes and `unrelated.rs` stay unchanged. Catalog and `lib.rs` gain `ITEM_REAL_FORMAT`. The item JSON is created. The content validator is the test stub. | `abc69a9` | PASS | Does not run the content-validator binary. The quality gate does, on the real tree. |
| Preparation failure leaves unrelated files unchanged | Automated, real `rustfmt` | `test_real_rustfmt_failure_leaves_sources_unchanged`: invalid Rust raises `RuntimeError`. The sibling bytes are unchanged. No `source.rs` is left in the source folder. | `abc69a9` | PASS | None for this boundary. Failure is before `stage`, so there is no journal to roll back. |
| Validation failure after publication restores the operation and leaves other files unchanged | Automated, real `rustfmt` | `test_real_formatter_validation_failure_restores_without_touching_the_module`: the stub validator sets `validated` and then fails. The test requires that flag, so a formatter error before publish cannot pass it. Catalog and `lib.rs` return to their pre-create bytes. The sibling is unchanged. The item JSON and the journal are absent. | `abc69a9` | PASS | The validator here is a stub that runs after real formatting and publish. |
| Process-crash recovery restores the operation's files | Automated | `test_process_exit_during_publish_recovers_on_restart`, inside the 23-test authoring file. | `abc69a9` | PASS | This is a process-crash journal. It does not fsync and is not a power-loss guarantee. |
| A second save between commit and the HTTP body cannot change the first response | Automated | `test_mob_save_response_matches_the_committed_bytes`: the second save lands in `before_authoring_http_response`. A draft that still names the previous revision gets HTTP 409. The first response's document and revision still hash to the committed bytes. | `abc69a9` | PASS | The seam is a test hook. Production is a no-op between unlock and the HTTP write. |
| Lock, recover, one-read revision, compare, and publish stay on one lock hold | Code review plus the authoring file | Item Lab create, save, and icon, and Mob Lab save, create, and duplicate, take `CatalogWriteLock` (Mob Lab also takes `CONTENT_WRITE_LOCK`), call `recover_authoring`, then read. `snapshot_item` and the Mob Lab response hash the bytes just read. `AuthoringOperation` does not take the catalog lock. HTTP is written after the `with` block exits. | `abc69a9` | PASS | Sprite-manifest saves, NPC Lab, and editors that skip the lock are still not on this journal. That migration is outside this change. |
| A stale draft is HTTP 409 and does not overwrite the newer file or replace the open draft | Automated plus client-source review | Server: the concurrency test above, and `test_concurrent_saves_from_one_revision_conflict`. Item Lab `app.js` sets `error.conflict` and does not call `boot` on that error, so the form stays. Mob Lab `api` throws before `state.doc` is replaced. | `abc69a9` | PASS | The 409 banner was not clicked in a browser in this cycle. |
| A retried create compares the full draft, including `equipment_slot` | Automated | `test_retry_compares_the_equipment_slot` and `test_finish_is_the_commit_and_retry_does_not_allocate_again`. | `abc69a9` | PASS | None for this rule. |
| Published and retired catalog ids are not reused | Automated | `test_retired_id_is_not_reused` and `test_concurrent_catalog_allocation_keeps_distinct_ids`. | `abc69a9` | PASS | Cleanup of a published id by deleting its registration and allocating it again is still forbidden. |
| Unexpected external bytes fail closed | Automated | `test_external_bytes_fail_closed`. | `abc69a9` | PASS | Recovery will not guess over bytes that match neither image. |
| Shared expectation: 10% quantity 1 at N=1000 is 100 units; a quantity range uses the mean | Automated | `node tools/test_authoring_chart.mjs` (`authoring chart ok`) and `test_chart_module`. Both labs load `tools/authoring_chart.js`. 10% of quantity 1–3 at N=1000 is 100 successes and 200 units. | `abc69a9` | PASS | This is the formula. It is not a hover in the Lab window, and it is not the future in-editor drop simulator. |
| Production loot rolls use `LootRng` and unbiased bounds | Automated | `cargo test -p purgatory-server production_rng -- --test-threads=8`: 2 passed. Quantity 1–2 hit both ends (each more than 4,000 of 10,000) with the mean within 0.05 of 1.5. The 10% band was 700..1300 of 10,000, not an exact quota. | `abc69a9` | PASS | These are generator diagnostics. They do not show a simulator screen. |
| Death, real id reservation, pickup, and restart keep the same item | Automated, ignored PostgreSQL | `cargo test -p purgatory-server --bin purgatory-server postgres_12c -- --ignored --test-threads=1`: 20 passed, 0 failed, 9.45s. The output names `postgres_12c_monster_death_pickup_survives_restart` and that test passed. It calls `begin_id_replenish` and `reserve_item_ids` for the live channel, kills `MONSTER_DEV_GUARANTEED_DROP`, checks quantity 2 of `ITEM_DEV_SAMPLE_SCRAP`, and after restart the same `ItemInstanceId` is in inventory. Disposable `postgres:18` container, database `purgatory_12a_test` on `127.0.0.1:5433`. The container was removed. | `abc69a9` | PASS | The default quality gate skips `#[ignore]` tests. This run is headless. It does not show the ground icon or the native client. |
| Workspace quality gate | Automated | `./scripts/check.ps1` exit 0: fmt `--check`, `cargo check --workspace`, clippy `-D warnings`, workspace tests, Mob Lab unittest 49 tests, content validator OK (maps=3, entities=7, monsters=4, equipment=8, defs=44). | `abc69a9` | PASS | Does not include the ignored PostgreSQL filter. That filter was run separately, above. |
| Hub buttons open Item Lab and Mob Lab from this worktree | Manual window | Not executed. | `abc69a9` | NOT RUN | Needs a click in the eframe Hub. This session has no input driver for that window. See the manual script. |
| Create an item in Item Lab (name, description, category, tag, icon), save, leave, and reopen | Manual window | Not executed on this commit. The `3f2cb13` page session is history only. | `abc69a9` | NOT RUN | Needs the Item Lab page after the Hub button. A rebuilt catalog id also needs a client and server restart before the game can load it. |
| Search and category filtering | Manual window | Not executed on this commit. | `abc69a9` | NOT RUN | Same Hub-to-page path. |
| Icon selection and import | Manual window | Not executed on this commit. The earlier session posted `/api/icons` and did not use the file chooser. | `abc69a9` | NOT RUN | The file chooser and the endpoint are different checks. Both are unrun for this commit. |
| Add the item to a development monster drop and save chance and quantity | Manual window | Not executed on this commit. | `abc69a9` | NOT RUN | Needs Mob Lab from the Hub, then a save. Do not leave that monster on a production map. |
| Reopen the drop table; WHERE USED names the same source | Manual window | Not executed on this commit. | `abc69a9` | NOT RUN | Same pages. |
| Both graphs, including hover of a precise N | Manual window | Not executed on this commit. The shared formula row above passed. | `abc69a9` | NOT RUN | Hover is a window check. The formula test does not replace it. |
| Native client: kill a development monster, see one ground drop with the right icon, pick it up, read the quantity, reconnect, same instance | Manual window plus a disposable database | Not executed. | `abc69a9` | NOT RUN | Needs the native winit client. Do not use the owner's player database. Do not place a development monster on a production map. The headless PostgreSQL row does not replace this. |
| Future in-editor runtime drop simulator | Out of V1 scope | Not part of this closeout. | `abc69a9` | DEFERRED | Explicitly outside V1. Do not treat `production_rng` or the chart formula as that screen. |

### Manual script

Build and start `target\debug\purgatory-dev-hub.exe` from this worktree. On the Hub dashboard, click **Item Lab**, then **Mob Lab**. Each PowerShell window should be this worktree. Item Lab health should report build `item-lab-v1` and this workspace path.

In Item Lab, create an item with a display name, description, category, tag, and a new 32×32 icon. Use the file chooser, not only a direct POST. Save. Select another item. Reopen the new item and confirm the values. Search by name. Filter by a category that hides it, then clear the filter.

In Mob Lab, add that item to a development monster's drop row. Save a 10% row with quantity 1, and a row with a quantity range. Reopen the monster. In Item Lab WHERE USED, the monster id should appear for that item. In both graphs, Inspect at N=1000 for the 10% quantity-1 row should read 100 expected units. Hover along the line and confirm the readout matches the same formula. A range uses the mean quantity. The graph is an average prediction, not a quota.

For the game, use a disposable database and test characters. If the new item's catalog id was compiled in, rebuild and restart the server and the client. On an isolated test map, place a development monster with a guaranteed drop and a known quantity. Kill it in the native client. The drop should appear once, with that item's ground icon. Pick it up. The inventory and the information window should show the quantity. Disconnect and reconnect. The same stack should remain, and its instance id should be unchanged in the existing diagnostics. Remove the monster from the test map when finished. Do not delete a published catalog registration to reuse the id.

## Existing equipment save

Branch `fix/item-lab-existing-equipment-save`. Implementation commit `2133bc3500b59e73f2f7ee795864e3d106a3dd48`, based on `c9e37f22845532e19cf21c3f5cf24ed595394227` (`origin/master`, the merge of pull request #125). Pull request #125 stays merged and was not changed. The checkout at `Purgatory\1` was not modified. Map line-ending dirt in `map.map1.json` and `map.map3.json` stayed unstaged.

### Regression

`validate_payload` required `item.*` for both creation and update. Saving the existing practice sword, ContentId 30006, label `equipment.debug.practice_sword`, failed with `label must match item.*`. The Technical label field was already read-only, so the editor could not change the label to satisfy that check.

An update now accepts `item.*` or `equipment.*`. Creation still requires `item.*`. The save addresses the existing file for that label. The stored label and ContentId are not rewritten. When `content_catalog.rs` maps the label, the stored ContentId must be that mapped id. A client-supplied ContentId that disagrees is rejected and writes nothing. Category, equipment slot, and stack-limit restrictions are unchanged. The write still goes through the authoring journal, and success is still `AuthoringOperation.finish`, under `CatalogWriteLock`.

### Repaired chart test and gate coverage

`test_javascript_uses_the_same_formula` looked in `tools/mob_lab/web/drops.js` for a formula string that now lives in `tools/authoring_chart.js`. That assertion is gone. The numbers are covered by `node tools/test_authoring_chart.mjs` (`authoring chart ok`), which Item Lab runs as `test_shared_chart_behavior` and Mob Lab already runs as `test_chart_module`. `test_both_labs_use_the_shared_chart` checks that both lab pages load `/authoring_chart.js` and call the shared functions (`filterItems` and `renderExpectationChart` in Item Lab, `dropExpectation` and `renderExpectationChart` in Mob Lab).

`scripts/check.ps1` and `scripts/check.sh` discover `tools/item_lab` and `tools/mob_lab` in separate Python processes, then run `node tools/test_authoring_chart.mjs`. A missing `node` or Python fails the gate. GitHub Actions `.github/workflows/quality.yml` runs `./scripts/check.sh`, so the same steps run on a pull request to `master`. The Hub gate list includes `item-lab` and `authoring-chart`, and also the existing `mob-lab` marker, which the previous five-step list had ignored.

### Automated results

These commands ran in this worktree on the implementation tree that became `2133bc3`, before this section was added. This section does not change that code.

| Command | Result |
|---|---|
| `py -3 -m unittest discover -s .\tools\item_lab -p test_*.py -v` | 7 tests, OK, 0.103s, exit 0 |
| `py -3 -m unittest discover -s .\tools\mob_lab -p test_authoring_save.py -v` | 29 tests, OK, 5.176s, exit 0. Includes the practice-sword save, every authored `equipment.*` identity, iron scrap 30009 and small potion 30011, identity and immutable-field rejection, HTTP 409, and create still rejecting `equipment.*`. |
| `node tools/test_authoring_chart.mjs` | `authoring chart ok`, exit 0 |
| `./scripts/check.ps1` | exit 0, about 179s. `HUB_GATE` PASS for fmt, cargo check, clippy, workspace tests, Mob Lab (55 tests, OK, 5.525s), Item Lab (7 tests, OK, 0.087s), shared drop chart, and content validation (`maps=3 entities=7 monsters=4 equipment=8 defs=44`). |

`py -3` printed `Could not find platform independent libraries <prefix>` and the suites still exited 0.

The save tests copy real item files into a temporary authoring tree and call `commit_item_save` or `POST /api/items/save`. They do not write the user's catalog. Existing proofs left in place: `test_finish_is_the_commit_and_retry_does_not_allocate_again`, `test_retry_compares_the_equipment_slot`, `test_stack_reduction_and_bad_icon_do_not_mutate`, `test_extra_authored_field_survives_save`, and `test_process_exit_during_publish_recovers_on_restart`.

No database migration, ContentId remapping, or protocol change. Protocol version stays 34.

### Browser acceptance

Item Lab was started from this worktree with `py -3 tools/item_lab/server.py --port 8777`. Health reported tool `item-lab`, build `item-lab-v1`, and workspace `C:\Users\Ariel\OneDrive\Desktop\Purgatory\item-lab-v1`. The process was stopped after these checks. The other checkout's Item Lab on port 8767 was left running and was not used.

The page writes below were development checks on this worktree. Before the implementation commit, the practice-sword presentation and both gameplay JSON files were restored, and the cloth-cap presentation plus `content/authoring/item_notes/30001.json` and `30006.json` were removed. Those files are not in the commit.

| Step | Result |
|---|---|
| Select practice sword 30006. Change the display name to `Practice blade` and the description to `A blunted practice blade.`. SAVE. | PASS. Banner returned to Ready. No `label must match item.*` error. ContentId stayed 30006. Technical label stayed read-only `equipment.debug.practice_sword`. |
| Select iron scrap, reopen the sword, then reload the page and select it again. | PASS. Name and description were still `Practice blade` and `A blunted practice blade.`. Identity unchanged. |
| Select cloth cap 30001. Set display name `Cloth cap` and description `A plain cloth cap.`. SAVE. Select another item, reopen the cap, reload, and select it again. | PASS. No label error. ContentId stayed 30001. Technical label stayed read-only `equipment.debug.cloth_cap`. Slot stayed headwear. The cap had no presentation file, so the icon key was set to `item.placeholder` to satisfy the existing icon field. The missing PNG is a separate content condition. It was not the label failure. |
| Two editor sessions on the sword. Session A saves description `Session A text`. Session B, still on the older revision, saves description `Session B text`. | PASS. Session B banner: `The item changed on disk. Reload before saving. The unsaved draft is still here. Reload to see the saved item, then reconcile.` The description field stayed `Session B text`. Disk description stayed `Session A text`. ContentId 30006 and label `equipment.debug.practice_sword` were unchanged, and the label stayed read-only. |
| New 32×32 RGBA icon through the file chooser | NOT RUN / NOT PROVEN. The owner later reported that icon upload worked, but the recorded evidence does not prove the browser file-chooser path specifically. Keep this distinct from direct/API upload success. |
| Hub buttons open Item Lab and Mob Lab | NOT RUN / NOT PROVEN as a dedicated acceptance observation. Source wiring and tool launch paths are present, but this row requires the actual Hub buttons. |
| Native client kill, ground drop, pickup, and reconnect | OWNER-REPORTED PASS on 2026-10-03. The owner reported the five-step practice-sword smoke worked through guaranteed drop, kill/pickup, and reconnect. This is manual owner evidence, not an agent-driven graphical session. |

## Limits

- Authoring recovery is a process-crash journal. It does not fsync and is not a power-loss or database disaster-recovery claim.
- `close_world_address` abandons an unmanifested remainder. Production channel stop uses `lose_all_authority`. The single-address method's callers are tests.
- Sprite-manifest saves and other labs are not on this journal.
- Protocol version stays 34. This cycle does not change the database schema or the protocol.
