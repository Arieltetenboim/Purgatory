# PHASE 6B REPORT — Interaction + UI Runtime

## Status

**GREEN (automated).** Manual game-window visibility is **not** user-confirmed.

Do **not** start Phase 6C until the user confirms E opens/rejects the replica interactable in the normal client.

## Quality gate

`./scripts/check.ps1` — **PURGATORY quality gate OK** (2026-08-29). `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`.

## Architecture introduced

- `Interactable` capability (kind marker only). Discovery via `World::interactable_near` (address + Transform range + capability). Visible ≠ interactable.
- Domain `InteractionSession` on `World` (Opened / Active / Updated / Closed). Not a UI window. Observer is `RuntimeEntityId`. One session per actor.
- Authoritative `try_open_interaction` / `close_interaction` / `maintain_interaction_sessions`. Client never supplies distance, validity, or outcome.
- Protocol **v5**: reliable `InteractOpen` / `InteractClose` and server Opened / Rejected / Updated / Closed. Snapshot `ReplicatedKind::Interactable` so the client can show advisory nearest-target.
- Client `UIRuntimeState` maps network results to presentation. `Action::Interact` (dev key E). Nearest replica interactable is advisory only.
- FOOTNOTE-stage developer fixtures: nearby switch, far chest, other-instance portal.

## Important APIs / types

- `purgatory_simulation::{Interactable, InteractionSession, World::try_open_interaction, INTERACT_RANGE}`
- `purgatory_protocol::{InteractOpen, InteractClose, ServerInteract, InteractRejectReason, ReplicatedKind::Interactable}`
- `purgatory_client::ui_runtime::UIRuntimeState`

## Tests added

- `crates/simulation/src/phase6b_tests.rs` — open nearby; reject range / non-interactable / wrong address; despawn and address change close; stale generation; maintain out-of-range; disconnect cleanup
- Protocol v5 goldens for Hello/Welcome and interaction envelopes; `InteractOpen` / `InteractRejected` roundtrips
- Snapshot builder includes visible interactables
- Client: E → `Action::Interact` edge; advisory nearest from replica (players ignored); `UIRuntimeState` Opened/Rejected/Closed stay visible
- Server `GameplayOwner`: nearby fixture Opens; far fixture Rejects `OutOfRange`; other player Rejects `NotInteractable`
- Interact control is lifecycle (not droppable RTT telemetry)

## Compatibility

- `PROTOCOL_VERSION` **5**. v4 peers are rejected at Hello.
- Player snapshot layout unchanged except optional kind=2 entities.
- Interpolation still presentation-only and still interpolates **players** only.
- Networking still uses `World::relevance_for(RuntimeEntityId)`.

## Deferred (intentional)

- Dialog / shop / inventory / crafting (later)
- Spatial AOI / scheduling / budgets (6D)
- Content registry / maps (6C)
- WindowManager / production UI
- Persistence (6E)

## Smoke (runtime visibility)

Nearby and far fixtures are part of `World::footnote_test_stage()` (server World and local client prediction World). Nearby stands on P0 at spawn+1.6. Replica-only interactable drawing. Overlay shows Interaction on every debug tab (nearest, last request, result, session). `E` is an edge `Action::Interact`; Interact control is lifecycle (not droppable RTT telemetry). One-shot `6B_INTERACT` traces cover input → send → server validate → client recv → UIRuntimeState.

**PHASE 6B READY FOR FINAL USER INTERACTION CHECK** — not `MANUAL RUNTIME PASSED`.

## Boundary

Work stopped at Phase 6B. **Do not begin Phase 6C.**
