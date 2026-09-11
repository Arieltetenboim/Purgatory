# N10 — NPC Dialogue Runtime Design

Status: **design contract; implementation not started**

Purpose: project the proven NPC Lab authoring model into the authoritative game runtime without creating a parallel interaction, animation, inventory, or quest system.

This document freezes the current N10 behavior and ownership contracts. Implementation should be split into the slices below and stop at each acceptance gate.

## 1. Existing foundations to reuse

N10 is integration work, not a greenfield dialogue engine.

Existing owners remain authoritative:

- `World::try_open_interaction` / `InteractionSession` own target validity, range, address and session lifetime.
- reliable `InteractOpen` / `InteractClose` and `ServerInteract` own the existing interaction control path.
- `UIRuntimeState` is the client-side projection of interaction state; it is not authoritative gameplay state.
- `NpcState` / `ReplicatedKind::Npc` already provide visible NPC runtime identity on the wire.
- Character Presentation / Animation Runtime own humanoid pose and authored `.anim` playback.
- Phase 11 Item/Inventory runtime owns item mutations.
- `content/authoring/npcs/` and NPC Lab remain the canonical authoring source.
- N4/N5 selection semantics remain the behavioral reference for conditions, ENTRY priority, pools and explicit continuation links.

Do not duplicate these systems inside a dialogue module.

## 2. First runtime proof NPC

The first social-dialogue proof uses:

`npc.welcome.traveler_stayed`

in the live FOOTNOTE development map.

The current magenta object at `[-17.8, -2.9]` is `entity.interactable.switch` / `Dev Switch`. N10 may replace that **live placement** with the proof Social NPC. The older `World::footnote_test_stage()` Phase 6B interaction fixtures remain intact for their focused tests unless a specific test requires an intentional update.

The proof NPC must be a real replicated NPC presentation, not the magenta generic-interactable quad and not the combat slime presentation.

## 3. Social NPC capability model

A Social NPC is one runtime entity with multiple capabilities:

- NPC / humanoid presentation identity;
- Transform / world placement;
- Interactable capability for authoritative `E` interaction;
- dialogue content identity;
- optional ordinary NPC simulation state only where genuinely required.

### Wire-kind rule

`ReplicatedKind` is currently a single visible kind. A Social NPC that is also interactable must project as `ReplicatedKind::Npc`, not `ReplicatedKind::Interactable`.

Therefore N10 must adjust classification/target discovery so that:

- server-side `Interactable` continues to determine whether interaction is legal;
- the client may treat a replicated NPC as an advisory candidate for `E`;
- the server still validates target/range/address/session;
- no new compound wire-kind or duplicate entity is introduced merely to express "NPC + interactable".

Visible kind must not become gameplay authority.

## 4. Authoring → runtime projection

Canonical source remains:

```text
content/authoring/npcs/<area>/npc.*.json
```

The simulation hot path must not interpret arbitrary authoring JSON directly.

N10 direction:

```text
authoring JSON
  -> validation
  -> runtime projection / typed definitions
  -> authored-id resolution
  -> server dialogue registry
```

Presentation-only fields remain client concerns.

The first runtime projection may be deliberately narrow, but it must preserve the path for all authored Welcome NPCs rather than hardcoding Traveler dialogue in Rust.

### Identity

Author-facing stable string ids such as `npc.welcome.traveler_stayed` remain canonical authored references.

Runtime-visible content must use the project's numeric ContentId catalog policy. N10a allocates the required NPC catalog row(s) rather than inventing another identity space.

## 5. Server dialogue ownership

Dialogue progression is **per player**, even when several players talk to the same physical NPC.

The server owns a dialogue session/projection associated with the already-authoritative interaction session. It must know at least:

- actor/player;
- interaction session id;
- target NPC runtime entity;
- target NPC content identity;
- active beat id;
- active line index / presentation point;
- whether the current beat has reached completion;
- pending selected-choice acknowledgement where needed.

Do not put dialogue progression into shared `NpcState`.

Two players may simultaneously be on different beats/lines/choices for the same NPC.

## 6. Selection and progression semantics

ENTRY selection follows the currently proven NPC Lab semantics:

1. ENTRY beats only;
2. all typed conditions must match;
3. pool eligibility applies;
4. highest integer priority wins;
5. authored JSON order breaks equal-priority ties.

Explicit `choice.next` transitions follow the authored continuation directly; they do not rerun ENTRY selection unless the current path ends and a future interaction begins.

### Beats and lines

One authored **Beat** is one NPC bubble and one progression unit. Its nonempty
`lines[]` are composed in authored order inside that bubble; they are not
separate advance steps.

For N10:

- text appears fully immediately;
- click on the NPC speech bubble advances/completes the current Beat;
- `E` also advances/completes the current Beat;
- future typewriter text is explicitly deferred.

When the current Beat has choices, its NPC bubble remains visible while choices appear.

When a Beat has no choices, advancing completes it and ends/follows only whatever behavior the authored contract explicitly supports. Do not invent an implicit quest transition.

## 7. Player choices

Choices are rendered in a separate world-space bubble above the **local player**.

Input support:

- mouse click selects a choice;
- Up/Down move the highlighted choice;
- `E` confirms the highlighted choice;
- movement remains locked while these keys are used for dialogue navigation.

After a choice is authoritatively accepted:

1. the choice bubble collapses to the selected player line;
2. the selected text remains above the player for approximately **0.5 s**;
3. the next NPC response/continuation is then shown.

This ~0.5 s delay is presentation timing only. Gameplay mutations associated with the accepted choice do not wait for the visual delay.

## 8. Dialogue UI placement

During an active dialogue:

- NPC line bubble is anchored above the NPC;
- player choice/selected-response bubble is anchored above the local player;
- the NPC's most recent line remains visible while choices are shown;
- world-space bubbles must remain readable near screen edges through client-side clamping/placement behavior;
- only the local player's dialogue UI is shown to that player.

N10 does not require a final visual-art pass. The first implementation must establish correct layout, readability and ownership; later polish may change bubble skin, typography and transitions without changing runtime semantics.

## 9. Input / UI mode contract

Opening Dialogue enters a dialogue UI mode.

While active:

- player movement input is locked;
- ordinary physics remain active;
- knockback/external movement can still move the player;
- mouse is available for UI interaction;
- Up/Down and `E` are routed to dialogue navigation/confirmation instead of movement/interaction opening;
- the player does **not** automatically rotate toward the NPC.

`ESC` is the general UI/interaction escape command. Dialogue is the first concrete consumer, but the design must allow the same command to close future UI modes rather than hardcoding ESC as "dialogue only".

Closing restores normal gameplay input immediately.

## 10. Local-only conversation presentation

Conversation presentation must not mutate the shared visible NPC merely because one player is talking to it.

For the observing client with the active dialogue:

- the NPC may locally face that player's character;
- line animation cues are local-only;
- future voice is local-only unless explicitly redesigned.

Other clients continue to see the ordinary shared NPC presentation.

This is a presentation override, not authoritative NPC facing/gameplay state.

## 11. Optional animation cue

`line.animation` remains optional.

On line start:

```text
active semantic line
  -> client resolves authored line
  -> optional animation id
  -> existing Animation Runtime / Character Presentation
```

Rules:

- `null` / absent cue is normal and keeps ordinary presentation, usually Idle;
- missing asset or playback failure falls back locally and never blocks dialogue;
- server protocol does not send filenames, keyframes, bone transforms or presentation timers;
- dialogue animation does not globally rotate/animate the NPC for other players;
- do not create a second NPC-only skeletal animation runtime.

## 12. Humanoid NPC presentation

The Social NPC proof must use the existing humanoid skeleton/presentation path and start in Idle.

N10 may add the smallest NPC presentation adapter necessary to instantiate a humanoid visual from NPC content. It should reuse existing skeleton, animation sampling, view and asset primitives rather than copying Player Character Presentation wholesale or retaining the combat sprite-sheet path as the social-NPC architecture.

The proof does not require final NPC art. A neutral humanoid visual using existing development assets is sufficient.

## 13. Completion, Heard and cancellation

**Closing a dialogue is not the same as completing a dialogue beat.**

`Dialogue Heard` / one-shot consumption is recorded only when the current beat reaches an authoritative completion boundary.

Completion boundaries:

- a beat with choices completes when a valid choice is authoritatively accepted;
- a beat without choices completes when the player advances past its final line;
- arriving at a continuation beat does not automatically mark that new beat heard.

Cancellation before completion does **not** record the current beat as heard.

Cancellation causes include:

- `ESC` / requested close;
- player death;
- target gone/dead/invalid;
- address change;
- authoritative interaction session leaving range.

A player who is knocked out of range therefore does not lose an unseen `once`/`lore` beat.

### No rollback rule

Already-authoritatively-applied effects are never rolled back merely because the conversation closes afterward.

Example: if a choice was accepted and its `Give Item` or `Set Fact` succeeded, that mutation remains. The next beat still remains unconsumed until that beat itself completes.

## 14. Damage, death and range

Damage by itself does **not** close Dialogue.

Ordinary hit reactions/combat may continue while movement input is locked.

If damage causes knockback:

- physics displacement is allowed;
- the existing authoritative interaction-range maintenance decides whether the session remains valid;
- if moved out of range, dialogue closes as cancellation without consuming the incomplete beat.

Player death closes dialogue immediately.

Target NPC death/disappearance closes dialogue immediately. Social NPC death is not expected in the initial proof but invalidation must fail safely.

## 15. Actions and player-relative state

Dialogue code must not directly mutate unrelated domains.

Choice actions dispatch through authoritative owners:

- `Set Fact` -> player narrative/fact owner introduced by N10 as the smallest durable per-character state required;
- `Mark NPC Met` -> same player-relative narrative state owner;
- `Give Item` / `Remove Item` -> existing Item/Inventory runtime;
- future actions -> their own authoritative domain owner.

`Has Item` / `Item Equipped` conditions read existing authoritative Item/Equipment state rather than mirrored dialogue booleans.

N10 must preserve atomic validation around a choice: the client sends semantic choice intent, and the server validates the active session/beat/choice before dispatching actions.

No arbitrary scripting language is introduced.

Persistence of the new narrative state is **not** required in N10 because Phase 12 remains deferred; however, ownership and state shape must be persistence-ready and must not be client-owned.

## 16. Protocol boundary

The current Phase 6B interaction protocol remains the outer session/lifetime contract.

Dialogue adds reliable semantic control for active dialogue state and player intents. Prefer compact identity such as:

- interaction session id;
- NPC content id where required;
- beat identity or resolved compact runtime index;
- line index;
- choice identity/index.

The exact encoding is an implementation detail for the relevant N10 slice, but these rules are frozen:

- client never sends dialogue text as authority;
- client never sends arbitrary actions;
- client never chooses the next beat directly;
- server validates progression;
- presentation assets/timers do not cross the network;
- dialogue events are lifecycle/control traffic, not droppable snapshot telemetry.

## 17. N10 implementation slices

### N10a — Runtime content + identity foundation

Goal: validated authored NPC dialogue can be projected into typed runtime definitions and looked up by stable runtime identity.

Includes:

- allocate numeric NPC ContentId for the proof NPC;
- define the smallest runtime dialogue definition/registry;
- project/validate authoring JSON without hot-path JSON interpretation;
- map proof NPC placement/content identity;
- tests for broken refs, duplicate ids and deterministic ENTRY ordering where owned here.

Gate: server can load and resolve Traveler dialogue definition by runtime NPC identity; no dialogue UI yet.

### N10b — Social NPC world/presentation proof

Goal: replace the live FOOTNOTE Dev Switch presentation with an interactable humanoid Social NPC at the proof position while preserving old focused fixtures.

Includes:

- Social NPC server entity/placement;
- NPC wins wire visual classification while retaining server `Interactable` capability;
- client advisory `E` targeting includes Social NPC;
- existing server authority still accepts/rejects interaction;
- humanoid skeleton presentation in Idle.

Gate: normal client sees humanoid NPC, approaches it, presses `E`, and receives a valid interaction session. No dialogue bubble yet.

### N10c — Dialogue session + NPC bubble

Goal: authoritative ENTRY selection and Beat progression become visible.

Includes:

- player-relative dialogue session state linked to InteractionSession;
- initial ENTRY selection from authoritative player state;
- NPC world-space speech bubble;
- click / `E` Beat progression;
- movement input lock and mouse UI mode;
- general ESC close path;
- cancellation semantics with no accidental Heard consumption.

Gate: Traveler conversation opens, shows one authored Beat per bubble, can be cancelled/reopened safely, and movement is locked only while active.

### N10d — Choices + continuation

Goal: authored choices work end-to-end.

Includes:

- player bubble above local player;
- mouse selection and Up/Down + `E` navigation;
- server validates choice against active session/beat;
- accepted player line remains ~0.5 s locally;
- explicit `next` continuation;
- current beat Heard completion at accepted choice.

Gate: at least one real Traveler branch can be completed and followed through authored continuation with mouse and keyboard.

### N10e — Narrative conditions + authoritative actions

Goal: authored state-dependent Welcome dialogue changes real runtime state safely.

Includes only currently authored condition/action vocabulary required for proof:

- facts;
- NPC Met;
- Dialogue Heard;
- Item Owned;
- Item Equipped;
- Set Fact;
- Mark NPC Met;
- Give Item;
- Remove Item.

Item mutations reuse Phase 11 owners. No generic quest manager.

Gate: the Workshop/Traveler package scenario can change selection after a completed interaction using authoritative per-player state and existing inventory ownership.

### N10f — Local dialogue presentation + multiplayer proof

Goal: dialogue-specific presentation remains private and multiplayer-safe.

Includes:

- local-only NPC facing toward the speaking player;
- optional `line.animation` through existing Animation Runtime;
- fallback on missing/no cue;
- two-client proof with independent dialogue progression on the same NPC;
- one player's bubble, choice highlight, facing override and animation cue do not leak to the other client.

Gate: two clients can concurrently speak to one Social NPC at different dialogue states with no shared-session or presentation corruption.

## 18. Final N10 manual acceptance

N10 is GREEN when the user can prove the following in the normal networked client:

```text
Humanoid Social NPC in Idle
  -> approach
  -> E
  -> movement locked + mouse usable
  -> NPC speech bubble
  -> click or E through Beats
  -> choices remain above player while NPC question remains visible
  -> mouse OR Up/Down + E selection
  -> selected player response shown briefly
  -> authoritative next response / state mutation
  -> optional local dialogue animation when authored
  -> ESC closes safely
  -> out-of-range cancellation does not consume unfinished dialogue
  -> death closes
  -> normal input restored after close
  -> two clients can use the same NPC independently
```

## 19. Deferred after N10

Not required for N10 GREEN:

- typewriter text;
- final speech-bubble art/animation polish;
- keyboard rebinding/settings UI beyond using existing action infrastructure where available;
- controller/gamepad dialogue navigation;
- voice playback/recording;
- lip sync / facial expression cues;
- shared/public conversation animations;
- NPC schedules/behavior trees;
- quest manager;
- narrative persistence to database/disk (Phase 12 concern);
- localization;
- full Welcome map/art replacement.

## 20. Stop / escalation conditions

Stop a slice instead of redesigning broadly if any of these are encountered:

- Social NPC requires changing the frozen general replication architecture rather than a local classification/targeting repair;
- authored state cannot be projected without introducing a second item/equipment owner;
- InteractionSession cannot safely remain the outer lifetime authority;
- local dialogue presentation would require authoritative/global NPC pose mutation;
- a slice expands into persistence, generic quest scripting, final UI framework or broad NPC AI.

Document the conflict first, then re-slice.
