# PHASE 9C REPORT — Ability command path

Status: **GREEN**. Root `PHASE` was `9C`. Protocol **v15**. **Phase 9D complete — see [`PHASE_9D_REPORT.md`](PHASE_9D_REPORT.md).**

## Implemented scope

Live command path:

`client J-key intent → AbilityActivate → server authorization → World::request_ability → ActionTable → 9B Active query/effects`

- Client sends ability id only for Independent Basic Strike (`selected = None`). No target, hit list, damage, range, facing, or effect result.
- Server validates connection ownership, pack existence, `AbilityGrantTable`, and `AbilityActivation` before `request_ability`. Alive / exclusive action / cooldown remain 9B-authoritative.
- Live player spawn attaches Health and grants `skill.basic.strike`. Default `World::spawn_player` still has no Health.
- 7.2 `nearest_health_target` skips `EntityKind::Player`, so live player Health does not turn workload Strike into player PvE.

No Attack/Hurt/Dead presentation, hotbar, NPC AI, PvP, prediction, or extra query shapes.

## Protocol / command contract

New envelopes (incompatible bump **14 → 15**):

- Client `AbilityActivate` tag **25** — `seq u32` + ability `ContentId` token + selected flag (`0` independent / `1` + `WireEntityId`)
- Server `Ability` Accepted tag **26** / Rejected tag **27**

`seq` is per-connection, equipment-style: first Accept `1`; `seq == 0` is a codec error; duplicate replays last Accept/Reject; stale/gap do not execute.

## Authorization

`World` owns `AbilityGrantTable` (`EntityId` → granted `AbilityId`s). Grants are dropped on despawn. Origins can later include progression/equipment/status without a skill-book system. The network handler checks grants; `request_ability` itself does not, so 9B direct-runtime tests stay valid.

## Files changed

Protocol: `ability.rs` (new), `message.rs`, `version.rs`, `lib.rs`, `wire_golden.rs`.

Simulation: `AbilityGrantTable`, World grant/revoke, `nearest_health_target` skips players, `phase9c_tests.rs`.

Server: live spawn Health + grant; `handle_ability_activate` → `request_ability`; handshake rate-limit.

Client: `J` edge → `AbilityActivateRequest`; network `NetworkEvent::Ability`.

Docs: this report, `PHASE`, `ROADMAP.md`, `ARCHITECTURE.md`, `DECISIONS.md` (ADR-0062), `PROTOCOL.md`, `CONTENT_PIPELINE.md`, `TEST_GATES.md`, `README.md`.

## Tests

- Simulation `phase9c_tests.rs` — grant drop on despawn; NPC nearest-health skips a Health-bearing player.
- GameplayOwner: empty swing; forward-query hit; unknown / ungranted / invalid activation / duplicate / cooldown / stale / dead / busy.
- QUIC `ability_activate_empty_swing_over_quic`.
- Client input: `KeyJ` + ability edge.

## Quality gate (focused)

Commands actually run:

```text
cargo fmt -p purgatory-protocol -p purgatory-simulation -p purgatory-server -p purgatory-client -p purgatory-bot-client
cargo test -p purgatory-protocol
cargo test -p purgatory-simulation --lib phase9
cargo test -p purgatory-simulation --lib ability::
cargo test -p purgatory-server --bin purgatory-server ability
cargo test -p purgatory-server --bin purgatory-server live_player_has_health
cargo test -p purgatory-client --bin purgatory-client ability_edge
cargo test -p purgatory-client --bin purgatory-client development_keys_map_to_actions
cargo clippy -p purgatory-protocol -p purgatory-simulation -p purgatory-server -p purgatory-client --all-targets -- -D warnings
cargo clippy -p purgatory-bot-client --all-targets -- -D warnings
```

All passed. Full workspace `./scripts/check.ps1` was not run (protocol bump is covered by protocol goldens + client/server version asserts in the focused set).

## Deviations

None vs the 9C contract. Grants live on World (not `PlayerBinding`) so NPCs/scripts can share the table later.

## Unresolved / 9D

Completed in 9D. See [`PHASE_9D_REPORT.md`](PHASE_9D_REPORT.md).
