# Death / Respawn — manual acceptance checklist

Automated tests cover persistence seams (HP milli + `health_revision`, logout/reconnect,
respawn half-max). They do **not** prove animation quality, modal UX polish, or
two-client timing. A product owner (or designated playtester) must run this checklist
on a build that includes Death/Respawn slices A–E.

## Preconditions

- Two native clients (`purgatory-client`) on the same server.
- Both players on the **same map** (default dev map is fine).
- Combat enabled (Basic Strike or live creature).
- Production death flow only: death modal → confirm → server `Respawn` (no DEV Respawn
  button; DEV Reset may remain for other diagnostics).

## Cooldown rule (A2)

Ordinary player **Respawn** must **not** reset ability cooldowns. In-flight actions and
effects are cleared; cooldown timers stay.

Authoritative owner: `World::clear_respawn_restoration_runtime` in
`crates/simulation/src/runtime.rs`, invoked from ordinary respawn restoration in
`crates/simulation/src/debug_action.rs` (`PlayerRestoreRuntime::Respawn`).

## Persistence rules (E)

- No separate persisted Dead flag. `Health.current <= 0` is Dead; storage is
  `characters.current_health` + `health_revision`.
- Logout or disconnect while Dead (HP 0) must restore Dead on next login (modal reopens).
- Respawn sets live HP to **half of max** and commits through the normal snapshot path.
- A stale snapshot with a **lower** `health_revision` must not overwrite a newer respawn HP.

## Two-client checklist

Run with **Client A** and **Client B** observing each other.

| Step | Action | Pass criteria |
|------|--------|----------------|
| 1 | Both enter world, same map | Both visible, movable |
| 2 | B damages A to lethal (or A suicides via combat) | A enters Dead presentation; A sees death modal |
| 3 | B observes A | B sees A in Dead activity (not idle locomotion) |
| 4 | A confirms respawn in modal | A waits on server; no duplicate sends on spam-click |
| 5 | After respawn | A spawns at **same map** authored default spawn (not arbitrary fixture); HP ≈ **50%** of max |
| 6 | Recovery window | A shows respawn recovery; movement/abilities gated briefly, then resume |
| 7 | B observes A after respawn | B sees recovery then normal locomotion; HP bar shows ~50% |
| 8 | Combat resume | A can strike B (and B can strike A) after recovery ends |
| 9 | Cooldown | If A used an ability shortly before death, respawn does **not** refresh that cooldown (strike still on cooldown if it was before death) |
| 10 | Logout while Dead (optional) | A dies, **does not** respawn, logs out; on login A is still Dead with modal |
| 11 | Persist respawn (optional) | A respawns to ~50%, logs out, logs in; HP still ~50% (not full, not zero) |

Record build id / git SHA, server map, and any visual defects separately from this
functional checklist. Do not mark visual acceptance GREEN from automation alone.
