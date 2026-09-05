# Phase 7.2 — Representative Gameplay Workload

**Status:** **complete / closed 2026-09-01.** Stop before Phase 7.3. Do not begin 7.3 until instructed.

Root `PHASE` remains `7.2`. Phase 6 remains GREEN (architecture closed). **7.1F** high-N harness issuance remains deferred to **7.3** (not fixed here). Localhost figures are **not** player-capacity claims.

Plan: [`docs/PHASE_7_PLAN.md`](PHASE_7_PLAN.md). Artifacts: `logs/load/capacity_72/`. Validation summary: `logs/load/capacity_72/summary_20260901_203801/phase72_summary.json`.

## What 7.2 is

Smallest authoritative NPC / action / health / timer / lifecycle workload on the Phase 6 spine, plus wire visibility (`ReplicatedKind::Npc`, protocol **v11**), harness presets, and 7.1 instrumentation attribution for NPC CPU. Measure only — no capacity ladder optimization, no AOI redesign, no harness issuance fix.

## Machine / build

| Field | Value |
|---|---|
| Date | 2026-09-01 |
| OS | Windows 11 Home 10.0.26200 |
| CPU | 13th Gen Intel Core i7-13650HX (14C / 20 logical) |
| RAM | ~15.7 GiB |
| Build | Release `purgatory-server` + `purgatory-load` |
| Helper | `scripts/capacity_ladder.ps1`, `scripts/capacity_72_validate.ps1` |
| Artifacts | `logs/load/capacity_72/` |
| Protocol | **v11** |
| Live UDP `PURGSTAT` | **schema 3** (not bumped) |

## Implemented scope

### Protocol (v11)

- `PROTOCOL_VERSION = 11`
- `ReplicatedKind::Npc = 4` for visible `EntityKind::Generic` + Transform (never Platform; Interactable/Portal unchanged)
- Client: distinct Npc draw/interp; excluded from E-interact and Portal routing
- ADR-0054; frozen v1–v10 goldens unchanged; v11 Hello/Welcome goldens added
- No AOI/fan-out/packer redesign

### Simulation workload

- Optional `NpcState` capability; `World::tick_npcs` after player movement (not inside `tick_player`)
- Deterministic bounded walk around home; inactive skip for active-% workloads
- Simulation-level `ActionRequest` + `ActionKind::Strike` (not a wire `ClientControl`)
- `apply_damage` via `set_health` (Health dirty independent of transform)
- `EffectKind::Pulse` scheduler ticks; death → delayed despawn + respawn (fresh `EntityId`)
- Production map-ready does **not** auto-spawn NPCs; population from tests / load-mode config

### Instrumentation

- Tick leaf `TickOwnerId::NpcActivity` / `npc_activity` (not folded into FOOTNOTE movement)
- `RuntimeStats` counters: NPC/actions/rejects/health/deaths/respawns/pulse
- File artifact `gameplay_workload.json` (+ NDJSON) ~1 Hz; **no UDP schema bump**

### Harness presets (canonical for 7.3)

| Preset | NPC count | active% | hotspot r | action period | pulse period | respawn delay | churn |
|---|---|---|---|---|---|---|---|
| `representative_light` | 8 | 50 | 4 | 60 | 90 | 45 | 0 |
| `representative_mixed` | 24 | 75 | 4 | 20 | 30 | 30 | 2 |
| `representative_dense` | 64 | 100 | 3 | 8 | 12 | 20 | 4 |

Seed defaults: light `72`, mixed `7202`, dense `7203`. Placement reuses `PURGATORY_LOAD_PLACEMENT=hotspot`. Ladder scenarios: `representative-light` / `representative-mixed` / `representative-dense`.

## Important files

| Area | Path |
|---|---|
| Protocol kind / version | `crates/protocol/src/snapshot.rs`, `version.rs` |
| Wire goldens | `crates/protocol/tests/wire_golden.rs` |
| Snapshot emit | `apps/server/src/network/replication.rs` |
| NPC / Strike / Pulse | `crates/simulation/src/npc.rs`, `runtime.rs`, `world.rs` |
| Tick order | `apps/server/src/network/gameplay.rs` |
| Workload artifact | `apps/server/src/network/tick_domains.rs`, `mod.rs` |
| Accounting leaf | `crates/common/src/capacity_accounting.rs` |
| Load config / presets | `crates/common/src/load_validation.rs`, `tools/bot_client/src/scenario.rs` |
| Load pressure | `apps/server/src/network/load_pressure.rs` |
| Correctness tests | `crates/simulation/src/phase72_tests.rs` |
| Validate helper | `scripts/capacity_72_validate.ps1` |
| ADR | [`docs/DECISIONS.md`](DECISIONS.md) ADR-0054 |

## Tests

- Simulation: spawn/inactive/deterministic motion; Strike validation + Health mutate; Pulse lifecycle; death/respawn
- Server: Generic Enter as `ReplicatedKind::Npc`; Phase 6 replication regressions green
- Protocol: v10 goldens frozen; v11 Hello/Welcome; unknown kind still rejected
- Quality gate: `cargo fmt --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace` — **PASS** 2026-09-01

## 7.2K validation (measure only)

`scripts/capacity_72_validate.ps1` — occupancy the harness can issue (4 / 8 / 8). No 384 / no optimization.

| Run | N | Duration | exit | tick p99 (ms) | util % | dominant | npc_activity mean (ms) | unattrib mean (ms) | npcs_active | actions started | health mut | pulse ticks | deaths |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| light functional | 4 | 20s | 0 | 0.359 | 0.41 | unattributed | 0.019 | 0.039 | 4 | 5 | 8 | 3 | 0 |
| mixed representative | 8 | 30s | 0 | 0.508 | 0.66 | **npc_activity** | 0.067 | 0.053 | 18 | 39 | 63 | 24 | 0 |
| dense smoke | 8 | 15s | 0 | 1.039 | 1.49 | **npc_activity** | 0.271 | 0.067 | 64 | 61 | 176 | 115 | 0 |

Notes:

- All three COMPLETE; overruns **0**; saturation class stayed `unknown_unattributed` (honest — no exclusive evidence for a named class).
- NPC / Strike / Pulse counters fire. Mixed and dense attribute CPU to `npc_activity` as intended.
- Short smokes did **not** reach death/respawn at these health budgets; lifecycle is covered by unit tests. Longer / denser 7.3 cells can exercise churn.
- Unattributed remainder exists but is small vs tick budget; NPC work is **not** silently folded into `simulation_movement`.

## Explicit non-goals (honored)

Inventory, loot, equipment, progression, quests, crafting, dialogue, skill trees, production combat, complex AI, pathfinding, content packs, UI expansion, persist vitals, AOI redesign, 7.1F harness-loop fix, 7.3 ladder/optimization.

## Deviations

None material vs plan. Optional Npc Enter golden vector was not added; Enter kind coverage is in server replication tests + kind roundtrip elsewhere.

## Unresolved / 7.3 inputs

- 7.1F harness high-N issuance (114/384) still deferred
- Death/respawn churn under representative presets needs longer or lower-HP cells before claiming lifecycle pressure on the ladder
- `representative_mixed` is the primary 7.3 candidate preset
- Dense is diagnostic only

## Boundary

7.2 is **closed**. **Do not begin Phase 7.3** until instructed.
