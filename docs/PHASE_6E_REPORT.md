# Phase 6E report — Character + persistence

## Implemented scope

- Protocol **v10**: `Hello.dev_login` after `client_build`. Version-first decode so v9 Hello bytes fail as `VersionMismatch`, not a login parse error. `DisconnectReasonCode::AlreadyConnected = 6`.
- `CharacterId` is a server-minted persistent player identity. `PersistentId` is unchanged and is not constructed from `CharacterId::raw()`.
- DEV login lookup (`DevLogin`): length `2..=32`, charset `[a-z0-9_.]`, reject `..`, path separators, leading/trailing `.`. No normalization. Exact string is the lookup key. Never a filesystem path.
- Crate `purgatory-persistence`: serialized identity allocation, file-backed characters (`char_{id:016x}.json`), schema version 1, revision-protected saves, crash-safe recoverable replacement (tmp / `.bak`). Simulation does not depend on this crate.
- Two-layer restore: `RestoreIntent` → logical destination → runtime placement (`ChannelId::DEFAULT` / `InstanceId::DEFAULT` as a temporary placement implementation). Authored map `restore` policy.
- Live QUIC enter path: identity → load/create → restore → occupancy reserve → place → spawn → bind → replication-ready → **Welcome**. Occupancy failure after reserve releases the reservation. Duplicate live Character rejects the new session.
- Generic persistent mutation: bump `persistence_revision` and `try_send` an owned snapshot to the worker. Portal acceptance is the first caller. No JSON or filesystem on the 30 Hz sim thread.
- Configurable bounded shutdown drain (`DEFAULT_PERSISTENCE_SHUTDOWN_TIMEOUT`, `PURGATORY_PERSISTENCE_SHUTDOWN_TIMEOUT_MS`).
- Connection Frontend DEV login field (default `dev.local`). Bots use unique `bot.{id:04}` logins.
- 6D ephemeral `GameplayOwner::attach` remains for unit tests.

## Important files

- `crates/common/src/identity.rs` — `CharacterId`, `DevLogin`, `RestoreIntent`
- `crates/protocol/src/message.rs`, `version.rs`, `tests/wire_golden.rs`
- `crates/persistence/`
- `crates/content/src/restore.rs`, map JSON restore policy
- `apps/server/src/network/{handshake.rs,gameplay.rs,persist.rs}`
- `apps/client/src/{frontend.rs,network/runtime.rs}`
- `tools/bot_client/src/session.rs`

## Tests added or changed

- DevLogin validation; Hello v10 goldens; frozen v9 Hello; v9 decode then VersionMismatch
- Identity store same-login / restart / different logins / login is not a path
- Character roundtrip, stale revision, corrupt fallback, tmp/bak recovery
- Restore SafePoint vs NonReenterable; placement uses DEFAULT channel/instance
- Occupancy reject + reconnect new EntityId; QUIC duplicate login AlreadyConnected

## Quality gate

Ran `./scripts/check.ps1` on 2026-08-30. Result: **PURGATORY quality gate OK**.

Commands actually executed, in order:

1. `cargo fmt --all -- --check` — pass
2. `cargo check --workspace` — pass
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass
4. `cargo test --workspace` — pass (including `purgatory-persistence` and protocol v10 goldens)
5. `cargo run -p purgatory-content-validator -q` — pass (`maps=2 entities=5 defs=7`)

Compiling is not completion; the full gate ran. Extended network soaks remain `#[ignore]` and were not run.

## Manual / runtime verification still required

1. First connect with `dev.local` spawns at footnote default restore point.
2. Disconnect and reconnect: same Character, new EntityId, restored map/point (not last X/Y).
3. Server restart: same login still maps to the same CharacterId under the runtime data root (`%LOCALAPPDATA%\Purgatory\` on Windows).
4. Two clients with the same DEV login: second is rejected (`Already connected`).
5. After a Portal to Map B, reconnect restores Map B safe point (DEFAULT channel), not last coordinates.
6. Client still treats empty replica / `MapId` 0 as uninitialized until Welcome + dest replica.
7. DEV login is never used as a filesystem path (files are `identity.json` and `char_*.json`).

## Deviations

- Windows file replacement is described as crash-safe recoverable (dest may be briefly absent; `.bak` recovery), not stronger atomic replace.
- Shutdown drain is named/configurable; default is 2 seconds, not an architectural invariant.
- `GameplayOwner::attach` remains for 6D unit tests without Character occupancy.

## Unresolved / risks

- DEV login is not an account system.
- Channel/Instance allocation is still DEFAULT-only placement.
- Inventory, quests, combat, and SQL persistence are out of scope.
- File store is single-writer per process; not a multi-process lock.

Work stopped at Phase 6E. **Do not begin Phase 6F.**
