# N10f — Local dialogue presentation and multiplayer handoff

Status: implemented on a stacked branch above N10e. Automated verification is
required, followed by the normal two-client networked proof.

## Implemented ownership

- `DialogueRuntime` exposes the active local session's presentation cue from
  the client-safe authored content projection. No facing, animation, or pose
  state was added to the network protocol or authoritative NPC state.
- `CharacterPresentationSet` remains the owner of humanoid pose playback. It
  applies an optional dialogue clip only to the locally selected NPC and
  restarts it when the local Beat/line presentation revision changes.
- Social-NPC facing is derived locally from the NPC and local-player positions
  while that client has an active dialogue. Closing the dialogue restores the
  ordinary NPC presentation.
- `DialogueAnimationCatalog` is a reusable client presentation catalog for
  authored `.anim` IDs. It scans the shared animation tree, rejects ambiguous
  duplicate IDs, skips invalid clips, and lets presentation fall back to the
  ordinary activity when a cue is null, missing, or invalid.
- Server gameplay definitions continue to omit presentation-only animation
  cues. The client-safe projection preserves each line's optional cue.
- `dialogue_talk.anim` and Traveler's first intro Beat provide the initial
  authored proof. The lookup and playback path is generic and contains no
  Traveler-specific branching.

## Manual two-client proof

Restart the server first because N10e narrative state and spawned NPCs are
intentionally memory-only.

1. Connect clients A and B to the same server and approach the same Traveler.
2. Open dialogue on both clients. If needed, progress one client so the clients
   are displaying different Beats or continuations.
3. On client A, verify the Traveler faces A and only A sees A's active bubble,
   choice focus, and authored dialogue animation.
4. On client B, verify the same Traveler faces B locally and only B sees B's
   active bubble, choice focus, and current dialogue state.
5. Continue or close dialogue on one client. The other client's bubble,
   progression, facing, and animation must remain unchanged.
6. Exercise a Beat with a null cue and confirm ordinary Idle presentation is
   used without interrupting dialogue progression.

## Deferred

- Narrative persistence remains deferred with Phase 12.
- No shared authoritative NPC pose, dialogue-presentation replication, voice
  runtime, or parallel animation system was added.
- N10 is ready for its final manual networked acceptance proof after N10e and
  this stacked N10f branch are reviewed in order.
