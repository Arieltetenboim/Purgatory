# Phase 10E — End-to-End PvE Proof

## Changed

- Reused the existing authoritative `PresentationOneShot` stream for NPCs:
  NPC Hurt now renders as a bright damage flash with an indicator bar.
- Reused replicated NPC Health and Enter/Leave lifecycle records:
  dead NPCs render as a distinct death state, while a newly entered respawn
  renders with a short respawn marker. This makes the death/despawn/respawn
  lifecycle visibly different from ordinary movement or teleport-like motion.
- Added one normal-session server integration test covering:
  player damage to creature, creature death, scheduled despawn and respawn,
  creature reacquisition and damage to player, player Dead, and player restore.
- No protocol, animation, AI, replication, or combat redesign was made.

## Tests

- `cargo test -p purgatory-server normal_session_pve_encounter_completes_both_directions_and_respawns`
- `cargo test -p purgatory-client npc_visual_cues_prioritize_lifecycle_states`
- `.\scripts\check.ps1` — **PASS** (`PURGATORY quality gate OK`)

## Manual proof

Still required in a two-client runtime session:

1. Player attacks the creature and observes the Hurt flash/indicator.
2. Creature visibly enters the death state, disappears through despawn, and
   returns with the respawn marker.
3. The respawned creature reacquires and attacks.
4. The player dies; both clients observe the persistent Dead presentation.
5. The player respawns and returns to the live presentation; repeat once.

## Remaining presentation limitations

- NPCs remain DEV placeholder rectangles rather than authored character
  skeletons/sprites.
- The death and respawn markers are deliberately minimal presentation proof,
  not authored VFX.

Phase 10E scope is complete; no later phase was started.
