# PHASE 10D-3 REPORT — Player Death Resolution Foundation

## Root cause / missing lifecycle seam

10D-2 established `Health.current <= 0` as the authoritative persistent Dead
condition, but there was no player lifecycle owner for the transition back to
Alive. The existing DEV reset only repositioned a player. It did not clear
action execution, cooldowns, physics/contact state, or the server input stream,
and it could not express a future Revive operation separately from Respawn.

## Implemented

- Added a shared authoritative player restoration path that:
  - restores Health;
  - clears active actions, ability runtime, cooldowns, target effects, and
    presentation oneshots;
  - clears velocity, grounding/contact, ignored-platform, and collider state;
  - preserves the existing `EntityId`;
  - dirties the transform and health replication domains.
- Added separate `World::revive_player_entity` and
  `World::respawn_player_entity` operations.
- Respawn uses the existing deterministic P0/FLOOR entry placement.
- Revive uses the shared reset path without changing position.
- Added a server-owned `PlayerDeathResolutionPolicy`; the current policy is
  deterministic Respawn at the next simulation lifecycle boundary.
- The server bumps the session input epoch and flushes stale input when
  Respawn succeeds. The existing snapshot epoch path causes local prediction
  to re-anchor, while transform/health dirtying carries the remote
  dead→alive transition.
- Kept the existing DEV reset behavior for players without a Health component.

## 10D-3 follow-up: remote dead→alive delivery

Manual two-client verification found that lethal Health was delivered but the
positive Respawn Health update was still suppressed by selective stranger
policy. `ObserverReplicationState` now remembers which already-known subjects
were last delivered as dead. Only the corresponding positive Health transition
is forced through the existing Update path; ordinary positive stranger Health
changes remain selective. Late Enter, baseline, and epoch-resync behavior is
unchanged.

## Tests

- Simulation restoration test: Health, placement, stable EntityId, action,
  cooldown, velocity, grounding, contact, and ignored-platform reset.
- Simulation Revive test: shared restoration without respawn placement and
  idempotent second trigger.
- Server policy test: stable EntityId, deterministic placement, Health restore,
  input queue flush, and input epoch rebase.
- Existing simulation and server suites remain covered by the normal tests.

## Remaining extension points

- Replace `PlayerDeathResolutionPolicy::Respawn` with a timer, player choice,
  checkpoint, or another policy without changing restoration mechanics.
- Add a real Revive authority/command and revive-specific semantics later.
- Add death penalties such as EXP loss only in a future policy layer.
- Client visual/UI treatment and corpse/persistence state remain out of scope.

## Boundary

No EXP loss, revive ability, timer, corpse state, UI, persistence change,
player→creature work, or Phase 10E work was started.
