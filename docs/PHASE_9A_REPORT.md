# PHASE 9A REPORT — Ability foundation contracts

Status: **GREEN**. Root `PHASE` = `9A`. Protocol **v14 unchanged**. **Phase 9B (Basic Attack) was not started.**

## Implemented scope

Contracts and a minimal executable path so later abilities reuse existing owners:

- Ability lifecycle owner = existing Action Runtime (`ActionTable`), not a parallel machine.
- Minimum `AbilityDefinition` (id, timing, targeting, ordered `effects[]`).
- Effect execution boundary: v1 `AbilityEffect::Damage` → `World::apply_damage`. Ability code does not `set_health`.
- Health / Alive / Dead ownership; cooldown as a World table, not a default entity component.
- Authoritative client→server→world flow locked in docs (no new wire tags).
- Gameplay→presentation cues map to Phase 8 Attack/Hurt oneshots; Dead is Health-derived.

7.2 `ActionKind::Strike` remains load-validation workload and is **not** the ability path.

## 1. Ability ownership

`World` `ActionTable` is the lifecycle owner (ADR-0046 amended, ADR-0061).

Pipeline (not stored): `request → validate → reject-or-start`.

Stored live phases: `Windup → Active → Recovery`. Terminal: `Completed / Cancelled / Interrupted / Failed`. At most one live action per owner (Windup/Recovery occupy the exclusive slot).

`ScheduledKind::AdvanceAction` advances live ability phases on the existing scheduler. `CompleteAction` still ends 6F Test / 7.2 Strike.

NPCs, bosses, and encounter scripts can call `World::request_ability` without a client envelope. Minigames are not required to use this table.

## 2. Ability definition

Runtime type in `purgatory-simulation` (`AbilityId` = `ContentId`):

- `id`
- `timing`: windup / active / recovery / cooldown ticks (0 skips that live phase)
- `targeting`: `None` | `Single { range }`
- `effects[]`: 1..=4, v1 `Damage { amount }`

JSON pack loading is **not** in 9A. Future shared JSON should mirror this shape (`content/shared/abilities/`). Basic Attack must be data on this type, not hardcoded combat constants. 7.2 `STRIKE_DAMAGE` / `STRIKE_RANGE` stay workload placeholders.

## 3. Effect boundary

`World::execute_ability_effect` is the only ability Health path. Instant effects are `AbilityEffect`. Duration-based `TempEffect` / `EffectKind::Pulse` stay separate (later status), not merged into `AbilityDefinition`.

## 4. Gameplay state ownership

| State | Owner | Default on entities |
|---|---|---|
| Health / MaxHealth | optional `EntityData.health` | Players spawn **without** Health |
| Alive / Dead | derived: `Health.current > 0` / `<= 0` | No Dead component. No Health ⇒ non-participant, not dead |
| NPC death churn | `NpcState.dead_pending` (7.2 despawn/respawn) | NPC capability only |
| Live ability | `ActionTable` keyed by owner | No slot until start succeeds |
| Cooldown | `CooldownTable` on World `(EntityId, AbilityId) → ready_tick` | Empty until an ability starts. Begins at successful start |

## 5. Networking contract (not on the wire in 9A)

Protocol **v14** unchanged. Future player envelope (when a later slice bumps protocol):

- **Client command:** ability id + optional target `EntityId`. No damage, no hit, no Health write.
- **Server-only:** Action live phase, cooldown table, definition resolution, effect execution.
- **Replicated:** existing Health domain. Semantic Attack/Hurt may reuse `ServerPresentationOneShot` (already v13). Do not replicate bones, clips, or cooldown in v1.
- **Presentation-only:** clip selection, sample time, skeleton.

No generalized prediction or lag compensation in 9A.

## 6. Presentation boundary

`GameplayPresentationCue::{Attack, Hurt, Dead}` → Phase 8 `PresentationOneShotKind` for Attack/Hurt. Dead has no Phase 8 activity yet; Health replica is the fact. Ability/combat must not name clips or bones. Animation Runtime was not modified.

## Tests

- `crates/simulation/src/ability.rs` — definition validation, phase pick, cues, cooldown table.
- `crates/simulation/src/action.rs` — Windup occupies exclusive slot.
- `crates/simulation/src/phase9a_tests.rs` — World request path, effect boundary, optional Health, Strike remains distinct.

## Files changed

Simulation: `ability.rs` (new), `action.rs`, `action_gate.rs`, `health.rs`, `scheduler.rs`, `runtime.rs`, `world.rs`, `lib.rs`, `phase9a_tests.rs`.

Docs: this report, `PHASE`, `ROADMAP.md`, `ARCHITECTURE.md`, `DECISIONS.md`, `CONTENT_PIPELINE.md`, `PROTOCOL.md`, `TEST_GATES.md`, `README.md`, `CHARACTER_ANIMATION_ARCHITECTURE.md`.

## Deviations

None material. `AbilityDefinition` lives in simulation (World cannot depend on content). Content JSON loader deferred to the slice that authors Basic Attack as data.

## Unresolved / 9B

Recommended 9B: author a data `AbilityDefinition` for Basic Attack, wire a player command (protocol bump only if required), execute on Active, emit Attack/Hurt presentation cues. Hit detection remains a later slice if range-target is insufficient.
