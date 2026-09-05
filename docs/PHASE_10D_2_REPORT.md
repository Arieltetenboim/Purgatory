# PHASE 10D-2 REPORT — Remote Death State

Status: **GREEN (automated)**. Root `PHASE` remains `10B`.
Protocol **v15 unchanged**. 10D-3 follows in
[`PHASE_10D_3_REPORT.md`](PHASE_10D_3_REPORT.md).

## Root cause

The remote presentation path was already complete:

`authoritative Health -> ReplicationRecord::Update.health -> ReplicatedEntity.health -> remote dead predicate -> PresentationActivity::Dead -> dead.anim`

The selective replication policy suppressed Health for stranger relationships.
Its silent catch-up committed the Health revision without sending the lethal
`Health <= 0` value to an existing observer. Late Enter baselines and epoch
resync baselines already included Health, so they were not the suppression point.

## Implemented scope

- Lethal authoritative Health transitions are immediately eligible for the
  existing replication Update path, including for selective strangers.
- Ordinary stranger Health coalescing remains unchanged.
- No separate alive/dead wire state was introduced.
- Late observer Enter and baseline/resync behavior remains unchanged.
- Remote presentation now has focused coverage proving `Dead` selects
  `dead.anim`.

## Changed

- `apps/server/src/network/replication.rs`
  - lethal Health policy exception
  - existing-observer, late-baseline, and epoch-resync tests
- `apps/client/src/character_presentation/tests.rs`
  - remote Health-derived Dead activity / clip test
- `crates/simulation/src/phase9e_tests.rs`
  - 10B exact ForwardQuery boundary regression check

## Verification

Passed:

```text
cargo test -p purgatory-server --bin purgatory-server selective_policy_emits_lethal_health_to_existing_observer
cargo test -p purgatory-server --bin purgatory-server dead_health_is_in_late_baseline_and_epoch_resync
cargo test -p purgatory-client --bin purgatory-client remote_dead_health_resolves_dead_activity_and_clip
cargo test -p purgatory-simulation --lib phase9e_tests
cargo fmt --all -- --check
./scripts/check.ps1
```

## Manual proof needed

Run rebuilt server and two clients: have Client A reach zero Health, then
confirm Client B receives the remote dead presentation and visibly shows
`dead.anim`. Also check a late observer and a channel/baseline resync while the
player is already dead.

## 10B side regression check

The exact authored range boundary is compatible: an NPC stopped at 1.5 units
still hit the player through the 1.5-unit ForwardQuery. No production 10B
change was made.

There remains a broader geometry distinction: approach uses center-to-center
Euclidean distance, while ForwardQuery uses a forward directional AABB with
`half_height`. A diagonal/vertical target can therefore satisfy approach stop
distance while remaining outside the authored hit volume. Resolving that would
change movement/combat geometry and is outside this narrow check.

## Remaining risk

Manual two-client visual/network proof remains required. Player restoration is
covered by the subsequent 10D-3 foundation report.
