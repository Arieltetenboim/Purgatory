# N10d — Dialogue choices and continuation handoff

## Implemented ownership

- `DialogueRuntime` on the server validates the active interaction session,
  Beat index, and choice index before accepting a choice.
- The client sends semantic choice intent only. Authored text, actions, and
  `choice.next` never come from the client.
- Accepted choices mark the current Beat Heard in transient per-player server
  state and follow the authored resolved continuation directly.
- Choice actions remain inert until N10e; no item, fact, NPC-met, quest, or
  persistence owner was added here.
- The client-safe content projection contains Beat text and choice labels only.
  The generic Text renderer accepts multiple blocks; the NPC SpeechBubble and
  player ChoiceBubble remain separate presentation consumers.

## Input and presentation

- One Beat is one NPC bubble. All authored `lines[]` are composed in order in
  that bubble and are not individual progression steps.
- Choices appear above the local player. Arrow Up/Down changes the highlight,
  `E` confirms it, and clicking a row selects and confirms that row.
- After server acceptance, the selected player response remains for 0.5 seconds.
  Any already-received authoritative continuation is buffered locally until
  that presentation-only delay ends.
- `ESC`, range/lifetime invalidation, and the existing InteractionSession remain
  the outer cancellation authority.

## Manual proof

1. Spawn any of the four Welcome NPCs from the DEV NPC Spawner and interact.
2. For Traveler's `intro`, verify the full Beat appears in one NPC bubble.
3. Choose `What's here?` with mouse; verify the selected response appears and
   then `intro_place` is shown.
4. Reopen and choose `I'm just passing through.` using Arrow Down and `E`; verify
   the `intro_passing` continuation.
5. Verify invalid/stale choice input does not change server progression and
   that movement unlocks after a terminal continuation.

## Deferred

N10e owns authoritative actions, facts, NPC Met, inventory mutations, and any
future persistence projection. N10f owns dialogue animation/facing/voice polish.
