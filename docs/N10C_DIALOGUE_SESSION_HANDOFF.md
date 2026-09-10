# N10c Dialogue Session Handoff

Status: paused before implementation on 2026-09-10.

Branch: `forge/n10c-dialogue-session-bubble`

## Completed foundation

- N10a, PR #39, merged as `84d171a0efe519a7766e06903421a6d78b9b1fb7`:
  authored NPC JSON is validated and projected into typed runtime dialogue definitions, and Traveler has numeric NPC `ContentId` `20001`.
- N10b, PR #40, merged as `4d3e3885d18640a982105ea064b53d24ee96f5f2`:
  Traveler is a live social NPC, uses the existing humanoid presentation path, and `E` opens the existing authoritative `InteractionSession`.
- Manual N10b proof confirmed that Traveler is visible with the development skeleton and that `E` reaches the interaction/debug open path.

No N10c runtime code has been committed. The current branch is based on the N10b merge.

## Investigation result

The existing runtime supports the authoritative half of the N10c trace:

```text
authored NPC JSON
  -> typed NpcDialogueDefinition
  -> ContentRegistry lookup by NPC ContentId
  -> World-owned InteractionSession
```

The missing architectural owner is production game text:

- the shipping client has a custom `wgpu` quad renderer but no reusable text renderer or text layout component;
- `egui` is development diagnostics only and cannot own production dialogue UI;
- the world renderer has a `MAX_QUADS` budget of 192, so one world quad per glyph would make game text compete with characters and world presentation;
- pre-rendering each dialogue line into a unique texture would solve only this proof and would make future HUD, labels, localization, choices, and dynamic text use different paths.

The initial idea of pre-rendering each authored dialogue line into a texture was rejected after review because it is dialogue-specific rather than a reusable text foundation.

## Pause decision

N10c is intentionally not marked GREEN.

An empty or placeholder speech bubble may be useful as a visual integration proof, but it does not satisfy the frozen N10c acceptance requirement that one authored NPC line be readable above the NPC. Do not merge a bubble-only proof under a claim that N10c is complete.

Before finishing N10c, create a reusable production text component that is not owned by Dialogue and does not depend on the development overlay.

## Text component contract to define

Resolve these requirements before choosing the renderer implementation:

1. Character coverage and localization
   - initial language set;
   - Unicode coverage and fallback fonts;
   - right-to-left and bidirectional text requirements;
   - missing-glyph behavior.
2. Typography
   - approved font assets and licenses;
   - style tokens such as family, size, weight, color, outline/shadow, and line height;
   - pixel-perfect versus scalable rendering expectations.
3. Layout
   - measurement, wrapping, explicit newlines, alignment, maximum width/height, overflow, and truncation;
   - resolution, DPI, render-scale, and window-resize behavior;
   - localization-safe sizing rather than English-string assumptions.
4. Composition
   - screen-space and world-anchored text using the same layout API;
   - clipping, layering, safe-area clamping, and bubble padding/tail placement;
   - layout output must expose the same bounds used for pointer hit testing.
5. Runtime behavior
   - static and dynamic strings;
   - glyph atlas creation/update, caching, batching, and explicit UI draw budget separate from `MAX_QUADS`;
   - deterministic fallback when a font or glyph cannot be rendered;
   - shipping build support with `--no-default-features`.
6. Content boundary
   - localization key versus authored literal text policy;
   - optional markup/rich-text policy;
   - ensure text and presentation lookup remain client-side while dialogue progression remains server-authoritative.

Recommended delivery boundary:

```text
production Text/Layout component
  -> reusable world-anchor UI primitive
  -> SpeechBubble component as first consumer
  -> resume N10c dialogue integration
```

The text component should have focused unit tests for measurement, wrapping, fallback, bounds, clamping, and resize/DPI invariants before Dialogue depends on it.

## Exact N10c implementation remaining

Once the text component contract is approved, resume with the following smallest coherent runtime change.

### Authoritative server

- Add a per-player active dialogue projection associated with the existing `InteractionSession`; do not put progression in shared `NpcState`.
- On a newly opened NPC interaction, resolve target entity -> NPC `ContentId` -> `NpcDialogueDefinition`.
- Select the ENTRY beat using the frozen NPC Lab semantics:
  1. ENTRY beats only;
  2. all typed conditions match;
  3. pool eligibility;
  4. highest integer priority;
  5. authored order for equal priority.
- Keep narrative facts, NPC-met state, Dialogue Heard state, and actions under the later N10e owner. N10c must not create persistence or a parallel inventory. Item/equipment conditions read the existing authoritative Item/Inventory and Equipment owners.
- Track interaction session id, actor, target, NPC `ContentId`, beat index, and line index.
- Validate every advance against the actor's current `InteractionSession` and active dialogue projection.
- Advance one authored NPC line at a time. If a final line has choices, leave it visible and wait for N10d; do not accept or simulate a choice in N10c.
- Remove dialogue projection whenever the owning `InteractionSession` closes because of request/escape, death, target removal/death, address change, range invalidation, or disconnect.
- Cancellation before completion must not consume once/lore state or mark Dialogue Heard.

### Reliable protocol

- Add client `DialogueAdvance { session_id }` intent.
- Add a server semantic active-line event carrying the interaction session id, target runtime entity, NPC `ContentId`, resolved beat index, and line index.
- Keep authored text, arbitrary actions, next-beat authority, animation assets, voice assets, and presentation timers out of client requests.
- Prefer client resolution of authored line content from semantic identity rather than transmitting authoritative progression as free-form text.
- Allocate new message tags, bump the incompatible protocol version from v24, and add round-trip, malformed-input, and frozen-vector coverage deliberately.

### Client dialogue/UI runtime

- Project server dialogue events into a local active dialogue state tied to `UIRuntimeState::Active` and the same interaction session id.
- Resolve NPC/beat/line through the canonical typed content registry; do not parse arbitrary JSON on the frame path.
- Feed the resolved line to the reusable `SpeechBubble`/text component.
- Anchor the NPC bubble above the active target, clamp it to the gameplay viewport, and hide it immediately when the interaction closes or the target leaves the local replica.
- Show only the local player's conversation UI.
- Route `E` and a click inside the bubble's returned hit bounds to `DialogueAdvance`.
- Add a general UI escape action for `ESC`; it closes the current interaction instead of being hardcoded as a dialogue-only command.
- Lock player movement input while dialogue is active on both client prediction and authoritative server input consumption. Suppress player intent without zeroing external velocity so gravity, collisions, knockback, and other ordinary physics continue.
- Restore gameplay input immediately on close and discard queued discrete edges so they do not replay after unlock.

### Speech bubble consumer

- The bubble skin, anchor, safe-area clamp, and pointer bounds may be implemented before typography is visually final.
- The bubble must consume the shared text layout result; it must not own a private font parser, glyph cache, wrapping algorithm, or input-coordinate conversion.
- The empty-bubble proof is temporary and must be replaced by readable authored text before N10c acceptance.

## N10c tests still required

1. Typed ENTRY selection tests matching NPC Lab priority, authored-order tie, condition, and pool behavior.
2. Server tests for Traveler open -> intro line event, per-player independence, valid advance, stale/wrong-session rejection, and all interaction close paths.
3. Cancellation/reopen test proving no once/lore or Dialogue Heard consumption before completion.
4. Protocol encode/decode, bounds, malformed payload, and version/golden tests.
5. Client dialogue projection tests for session matching, line resolution, close cleanup, movement lock, escape routing, and target disappearance.
6. Text/layout and bubble tests described above, including hit bounds and viewport clamping.
7. Shipping client build/test with `--no-default-features` so the result cannot depend on `egui`.
8. Manual network proof: `E` opens Traveler dialogue, authored text appears above Traveler, `E` and bubble click advance, `ESC` closes, movement is locked only while active, and cancellation/reopen is safe.

Run focused package tests first, then affected content/protocol/server/client tests. Run `scripts/check.sh` last and report the known unrelated formatting drift separately if it still exists.

## Explicit non-goals at this stop

- N10d choices, choice navigation, choice confirmation, and continuation;
- N10e actions, facts, NPC-met mutation, Dialogue Heard mutation, or persistence;
- N10f dialogue animation/voice playback and local facing polish;
- a second interaction system, inventory system, animation runtime, scripting language, or quest manager.

## Resume point

The next session should begin by collecting/approving the text component requirements above. Do not restart the NPC/content/interaction investigation unless `master` changed in those ownership areas. After the text contract is fixed, implement the production text component first and then resume the documented N10c trace. Stop again before N10d.
