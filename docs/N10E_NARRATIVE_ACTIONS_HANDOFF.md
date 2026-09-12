# N10e — Narrative state and authoritative actions handoff

Status: implemented and manually verified on PR #43. N10f is implemented in
the stacked PR #45. The current combined system reference is
[`NPC_DIALOGUE_RUNTIME.md`](NPC_DIALOGUE_RUNTIME.md).

## Implemented ownership

- `NarrativeRuntime` is the server-owned, per-player owner of boolean Facts,
  NPC Met, and Dialogue Heard. It is separate from transient dialogue-session
  progression and from item state.
- `DialogueRuntime` validates the current session, Beat, and choice and creates
  a choice plan without mutating state. Only a successfully dispatched plan is
  committed to its authored continuation or completion.
- `dialogue_actions` resolves and preflights the complete authored action list,
  then dispatches actions in JSON order:
  - Set Fact and Mark NPC Met go to `NarrativeRuntime`;
  - Give Item and Remove Item go through new `World` inventory-owner APIs.
- Item Owned and Item Equipped conditions read the existing authoritative
  inventory/equipment state. They are not mirrored as dialogue flags.
- Accepted choices and completed no-choice Beats mark Dialogue Heard. Closing
  an incomplete continuation does not mark that continuation Heard.
- `item.package`, `item.welcome.road_marker_cloth_bundle`, and
  `item.welcome.watch_signal_lantern` are now validated shared item content.
  Any missing dialogue item reference fails content loading clearly.

## Welcome proof bootstrap

Each live player receives the three temporary Welcome facts documented by the
canonical Traveler JSON:

- `welcome.workshop.package_needed=true`;
- `welcome.workshop.package_at_inn=true`;
- `welcome.workshop.package_delivered=false`.

The generic narrative owner does not know these labels. The seed is isolated at
the server player-entry integration boundary so a future persisted character
snapshot can replace it without changing dialogue evaluation.

## Manual proof

Restart the server first because N10e state is intentionally memory-only.

1. Speak to the placed Traveler and complete `intro` with any choice.
2. Speak to the Traveler again. The package Beat should now win ENTRY
   selection. Choose the response that takes the package.
3. Confirm `item.package` appears in the authoritative Inventory debug state.
4. Close the continuation with `ESC`. The package and changed fact must remain;
   cancellation does not roll back the accepted choice.
5. In the DEV NPC Spawner choose
   `npc.welcome.workshop_craftsperson`, spawn it at the player, and interact.
6. Choose the package handover. The package must leave Inventory and reopening
   relevant NPC dialogue must select from the delivered state rather than the
   waiting/package state.

## Deferred

- Narrative persistence remains deferred with Phase 12.
- No quest manager, arbitrary scripting runtime, second inventory, or client
  action authority was added.
- N10f owns local-only facing/animation cues and the two-client presentation
  isolation proof.
