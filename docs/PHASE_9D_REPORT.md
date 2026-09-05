# PHASE 9D REPORT — Ability → Character Presentation

Status: **GREEN**. Root `PHASE` = `9D`. Protocol **v15 unchanged**. **Phase 9E was not started.**

## Implemented scope

Connect authoritative Phase 9 combat to Phase 8 Character Presentation:

`Ability execution / Damage / Health`
→ semantic Attack / Hurt / Dead
→ existing oneshot + animation runtime

Combat behavior is unchanged. Presentation does not depend on `ForwardQuery`, `SelectedEntity`, hit count, or `skill.basic.strike` identity.

## Presentation contract

| Cue | Source | Wire / replica | Character Presentation |
|---|---|---|---|
| **Attack** | Successful `request_ability` (execution start) | `ServerPresentationOneShot` kind Attack | oneshot overlay |
| **Hurt** | Authoritative non-lethal Health loss via `apply_damage` | `ServerPresentationOneShot` kind Hurt | oneshot overlay |
| **Dead** | `Health.current <= 0` | replicated Health domain | persistent `PresentationActivity::Dead` |

Rules:

- Ability execution → Attack even with zero hits (empty swing).
- No damage → no Hurt.
- Lethal damage clears transient oneshots; Dead comes from Health, not a oneshot.
- Already-dead damage does not restart Hurt.
- Precedence in Character Presentation: **Dead > Hurt/Attack oneshot > locomotion**.
- Dead placeholder clip = existing Hurt clip (no new art).

## Files changed

Simulation: `request_ability` starts Attack oneshot; `apply_damage` starts Hurt / clears on lethal; `RuntimeEvent::PresentationOneShotStarted|Cleared`; `phase9d_tests.rs`.

Server: fanout presentation runtime events after Accepted ability and each sim tick (same `ServerPresentationOneShot` path as A5 DEV).

Client: `PresentationActivity::Dead`; `resolve_presentation_activity`; replica Health drives Dead for local and remote players.

Docs: this report, `PHASE`, `ROADMAP.md`, `ARCHITECTURE.md`, `DECISIONS.md`, `TEST_GATES.md`, `README.md`, `CHARACTER_ANIMATION_ARCHITECTURE.md`.

## Tests

- `phase9d_tests.rs` — Attack on execute; empty Attack; Hurt on damage; no Hurt without damage; lethal clear; already-dead; generic ability id.
- Server — Attack broadcast on activate; Hurt fanout on hit.
- Client — Dead overrides; duplicate oneshot events; clear kind 0.

## Quality gate (focused)

Commands actually run:

```text
cargo fmt -p purgatory-simulation -p purgatory-server -p purgatory-client
cargo test -p purgatory-simulation --lib phase9
cargo test -p purgatory-server --bin purgatory-server ability
cargo test -p purgatory-client --bin purgatory-client dead_overrides
cargo test -p purgatory-client --bin purgatory-client oneshot
cargo test -p purgatory-client --bin purgatory-client local_and_remote_dead
cargo test -p purgatory-client --bin purgatory-client a5_
cargo test -p purgatory-client --bin purgatory-client activity_maps_to_presentation_view
cargo clippy -p purgatory-simulation -p purgatory-server -p purgatory-client --all-targets -- -D warnings
```

All passed. Full workspace `./scripts/check.ps1` was not run (no protocol bump).

## Manual / runtime still required

Two-client visual check: A presses J → both see Attack; empty vs hit; lethal → persistent Dead.

## Unresolved / 9E

Not started. Recommended 9E: authored death clip (replace Hurt placeholder), optional Hurt interrupt polish, NPC presentation if non-player combatants need skeleton draw. Do not add particles/audio/hit-stop/prediction.
