# N10 Runtime Implementation Plan

Status: implementation plan derived from `NPC_DIALOGUE_RUNTIME_DESIGN.md`; no engine changes in this branch.

## Scope rule

Implement one slice at a time. Each slice should reuse current owners and stop when its gate is proven. Do not carry unfinished UI/runtime work into the next slice merely for convenience.

## N10a — Runtime content + identity

Trace:

```text
NPC authoring JSON -> validation/projection -> typed runtime registry -> NPC ContentId lookup
```

Likely owners:

- `crates/content`
- `crates/common` numeric content catalog
- `content/authoring/npcs`
- server startup/content load path

Acceptance:

- Traveler has allocated numeric NPC ContentId;
- server resolves Traveler's validated dialogue definition by NPC runtime identity;
- no hot-path arbitrary JSON evaluation;
- malformed/duplicate runtime dialogue content fails load clearly;
- no client or protocol behavior change yet.

Stop if projection requires a generic scripting engine or a parallel content registry.

## N10b — Social NPC + humanoid Idle

Trace:

```text
map placement -> server entity spawn -> replication kind -> client NPC presentation -> E target -> InteractionSession
```

Likely owners:

- server entity/placement loader
- simulation runtime spawn capability wiring
- replication snapshot/classification
- client advisory target discovery
- Character Presentation / NPC presentation adapter

Acceptance:

- live FOOTNOTE proof position no longer shows Dev Switch magenta quad;
- Traveler appears as humanoid skeleton in Idle;
- Traveler remains server-authoritatively interactable;
- E near Traveler opens existing InteractionSession;
- E out of range is still rejected by server;
- old Phase 6B focused fixtures/tests remain valid.

Stop if a second entity must be spawned only to represent interaction or presentation.

## N10c — Dialogue session + NPC bubble

Trace:

```text
InteractionSession Opened -> server ENTRY selection -> active beat/line -> reliable semantic update -> local NPC bubble
```

Likely owners:

- server gameplay/dialogue owner
- protocol reliable control
- client dialogue/UI runtime
- input routing

Acceptance:

- first eligible authored Traveler beat selected per player;
- one authored NPC line visible at a time above NPC;
- E and mouse advance lines;
- movement input locked while dialogue active;
- ordinary physics still run;
- ESC uses general UI escape path;
- cancel/reopen before completion does not consume once/lore dialogue;
- death/target gone/address/range invalidation closes safely.

Stop if dialogue lifetime diverges from authoritative InteractionSession lifetime.

## N10d — Choices + continuation

Trace:

```text
active final line -> choices -> local navigation -> choice intent -> server validation -> accepted choice -> continuation
```

Acceptance:

- choices appear above local player while NPC last line remains visible;
- mouse selection works;
- Up/Down changes selected choice, E confirms;
- player movement remains locked while those inputs navigate UI;
- server rejects stale/invalid session, beat or choice intents;
- selected player response remains visible ~0.5 s locally;
- explicit authored `next` continuation is followed;
- accepted choice marks the completed beat Heard exactly once.

Stop if client needs to send dialogue text/actions/next-beat authority.

## N10e — Narrative state + actions

Trace:

```text
choice accepted -> authoritative action dispatch -> narrative/item/equipment owners -> next selection reflects resulting state
```

Acceptance:

- authoritative per-player facts;
- NPC Met and Dialogue Heard conditions;
- Item Owned/Equipped conditions read Phase 11 state;
- Set Fact / Mark NPC Met mutate narrative state;
- Give/Remove Item use existing Item/Inventory runtime;
- cancellation never rolls back an already accepted action;
- no persistence requirement yet;
- no quest manager or arbitrary script runtime.

Proof scenario: Traveler/Workshop package flow changes correctly depending on per-player state and inventory.

Stop if action execution bypasses the owning subsystem.

## N10f — Local presentation + multiplayer

Trace:

```text
active local line -> local facing/animation cue -> existing Character/Animation Presentation
```

Acceptance:

- speaking client locally sees NPC face toward its player;
- other clients do not receive that dialogue-facing override;
- optional line animation plays only for speaking client;
- null/missing/failed cue falls back without affecting progression;
- two clients can simultaneously speak to the same NPC at different beats/lines;
- bubbles/choice focus/animation/facing do not leak across clients.

Stop if dialogue presentation requires shared authoritative NPC pose mutation.

## Test strategy

For each slice:

1. closest owner/unit behavior test;
2. affected package/crate tests;
3. protocol golden/roundtrip only when wire changes;
4. normal networked manual proof for visible behavior;
5. broader workspace gate only when justified.

The existing unrelated `cargo fmt --check` drift in `crates/common/src/identity.rs` and `crates/common/src/lib.rs` remains a separate repository hygiene issue until fixed independently.

## Final N10 GREEN proof

A normal networked player approaches the humanoid Traveler, presses E, reads authored NPC lines, uses either mouse or keyboard to navigate choices above the player, sees the selected response briefly, reaches authoritative continuation/state changes, can safely ESC/cancel without losing incomplete dialogue, and two clients can do this independently on the same NPC. Optional dialogue animation is local-only.
