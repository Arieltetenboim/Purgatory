# N10c Dialogue Session Handoff

Status: implemented; automated verification is required, followed by the normal
networked visual proof before N10c is accepted.

Branch: `forge/n10c-dialogue-session-bubble`

## Foundation

- N10a, PR #39, merged as `84d171a0efe519a7766e06903421a6d78b9b1fb7`:
  canonical NPC JSON is projected into typed runtime content and Traveler owns
  numeric NPC `ContentId` `20001`.
- N10b, PR #40, merged as `4d3e3885d18640a982105ea064b53d24ee96f5f2`:
  Traveler is a live social NPC using the existing interaction and humanoid
  presentation paths.
- Text v0, source commit `78dcc14a44a43ae1e87a84897df4b0d1d65ab2ae`:
  reusable native screen-space glyph-atlas text, independent of Dialogue.

## Ownership boundaries

N10c keeps three systems separate:

1. The server dialogue runtime owns authoritative ENTRY selection and per-player
   Beat progression tied to the existing `InteractionSession`.
2. The client dialogue projection validates semantic line identity and resolves
   readable content from the typed shared `ContentRegistry`.
3. `SpeechBubble` owns presentation geometry, world anchoring, viewport clamping,
   and pointer bounds while generic Text v0 owns font metrics, wrapping, glyph
   caching, and the text draw pass.

Text has no NPC, dialogue, or input concepts. `SpeechBubble` does not parse fonts,
cache glyphs, or implement wrapping. Neither presentation system owns dialogue
authority.

## Implemented N10c trace

```text
E -> InteractionSession -> server ENTRY selection -> protocol Beat identity
  -> client typed-content lookup -> SpeechBubble -> Text v0
```

- ENTRY selection preserves the NPC Lab rules: ENTRY beats only, typed condition
  match, pool eligibility, highest integer priority, and authored-order tie break.
- Narrative facts, NPC-met state, Dialogue Heard mutation, actions, and
  persistence remain deferred. Item/equipment conditions read their existing
  authoritative owners.
- Reliable protocol v25 adds `DialogueAdvance { session_id }` and an active-Beat
  event containing only session, target, NPC `ContentId`, beat index, and line
  index. Its retained line index is the zero compatibility anchor. Authored text
  and arbitrary actions are not accepted from the client.
- The local client resolves the Beat from shared typed content, composes its
  authored `lines[]` once during loading, displays one NPC
  bubble, and clears it on interaction close, session mismatch, screen change, or
  target disappearance.
- `E` or a click inside the bubble advances the Beat. `ESC` uses the general
  `InteractionSession` close path.
- Dialogue movement input is neutralized on client prediction and authoritative
  server consumption without zeroing external physics velocity.
- A Beat with authored choices remains visible. Choice rendering and
  selection are N10d and are not implemented here.

## Acceptance verification

Automated coverage must include content selection/projection, protocol round-trip
and frozen vectors, server open/advance/stale-session/cleanup/input-lock behavior,
client session matching/content resolution/cleanup, Text wrapping/metrics, bubble
clamping/hit bounds, and a shipping `--no-default-features` client build.

Manual network proof:

1. Approach Traveler and press `E`; the authored intro appears above Traveler.
2. Resize/move the camera and verify the bubble stays in the gameplay viewport.
3. Verify `E` and a click inside the bubble route an advance. Traveler's current
   intro ends in choices, so it deliberately remains visible until N10d.
4. Verify `ESC` closes immediately and movement is locked only while active.
5. Cancel/reopen and confirm no lore/once/Heard state was consumed.

## Explicit non-goals

- N10d choice presentation, navigation, confirmation, or continuation;
- N10e actions, facts, NPC-met/Dialogue Heard mutation, or persistence;
- N10f animation, voice, facing polish, or final speech-bubble art;
- a second interaction, inventory, animation, scripting, or quest system.

Stop before N10d and wait for review after the N10c proof.
