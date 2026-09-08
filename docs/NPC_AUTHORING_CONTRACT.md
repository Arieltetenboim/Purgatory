# NPC Authoring Contract

Status: FORGE N0 authoring contract

## Purpose

Define the smallest durable source format for authored NPC content before NPC Lab UI or NPC runtime integration is implemented.

This contract exists so real Welcome NPCs can be authored now without inventing a quest manager, scripting language, generic AI framework, or final runtime architecture.

## Core authoring flow

```text
NPC Lab (local Web UI, later)
    -> canonical versioned JSON under content/authoring/npcs/
    -> authoring validation
    -> runtime projection/registry later
```

The repository JSON document is the source of truth. NPC Lab must not own NPC content in a private database.

The N0 files are **not scanned by the current runtime content loader**. Runtime integration belongs to N10 and must be re-sliced after the authored model is proven on several real NPCs.

## Identity

NPC authored identity uses the existing stable string `ContentId` convention.

Example:

```text
npc.welcome.traveler_stayed
```

Display name, role, location, presentation, and authored text may change without requiring the stable authored identity to change.

Do not use runtime `EntityId`, numeric `MapId`, `ChannelId`, or `InstanceId` as authored NPC identity.

## Canonical N0 document

One authoring document represents one NPC.

The v1 document intentionally keeps design context and candidate interaction content together while the model is still being discovered. Runtime/server/client projections may split later without changing the authoring source of truth.

Top-level shape:

```text
schema_version
id
design
relationships
interaction
notes
```

Only fields required by current Welcome design are included.

## Design block

`design` is authoring context. It is not automatically shipping/runtime data.

Candidate fields in v1:

- `working_name`
- `display_name`
- `role`
- `area`
- `tags[]`
- `background`
- `personality[]`
- `speech_style[]`
- `gameplay_purposes[]`
- `narrative_purposes[]`

`display_name` may be `null` while a character is still being designed.

NPC Lab may later expose these as structured forms while preserving the same canonical document.

## Relationships

`relationships[]` contains authored references to other NPC `ContentId`s plus design notes.

N0 does not define a reputation, affinity, friendship-score, or runtime relationship system.

A relationship entry means only:

```text
this NPC authoring document refers to another NPC and records useful design context
```

Broken referenced NPC IDs must eventually fail authoring/content validation once the referenced NPC exists in the authored pack. During early design, an explicitly marked unresolved relationship may remain a design note rather than a runtime reference.

## Interaction model

N0 defines only enough shape to prove that conversations can be represented without free-form code.

`interaction.beats[]` contains conversation beats.

Each beat may contain:

- stable local `id`
- author-facing `title`
- integer `priority`
- `conditions`
- NPC `lines[]`
- player `choices[]`
- optional authoring notes

A beat is an authored conversation unit, not a quest stage.

Finishing a beat does not automatically advance NPC progression.

## Conditions

N0 conditions are declarative data, never JavaScript or arbitrary expressions.

The only condition form frozen in N0 is a boolean authored fact reference:

```json
{ "fact": "welcome.workshop.package_needed", "equals": true }
```

This is deliberately smaller than the expected final vocabulary.

Future slices may add typed conditions such as:

- item owned
- item equipped
- dialogue heard
- NPC met
- world state
- NPC activity

only when real authored content requires them.

N0 does **not** define where all facts live at runtime, how they are persisted, or which subsystem owns their mutation.

## Dialogue lines

A line contains authored text plus optional presentation references:

```text
text
voice
animation
```

`voice` and `animation` are optional logical asset references, not filesystem paths.

N0 records the references but does not implement playback, voice generation, animation dispatch, localization, or dialogue UI.

## Player choices

A choice may contain:

- local `id`
- visible `text`
- `next`

`next` is either another beat local ID or `null` to end the current conversation path.

A player choice does **not** imply a divergent quest outcome. It may simply expose optional context, lore, tone, or conversational freedom.

N0 deliberately does not add arbitrary gameplay actions to choices.

Gameplay mutations such as item transfer, equipment change, health modification, movement, or world transition must later route through the authoritative domain owner rather than being performed by an NPC script.

## Dialogue pools

The v1 document may declare pool metadata for authored beats so the content model can represent optional/repeatable material before N5 implements editing/selection behavior.

Allowed N0 labels are design metadata only:

- `mandatory`
- `once`
- `repeatable`
- `rare`
- `lore`

No runtime random-selection semantics are frozen by N0.

## Fact/reference naming

Facts use stable authored string IDs scoped by meaning rather than by numeric quest stage.

Examples:

```text
npc.welcome.traveler_stayed.met
welcome.workshop.package_needed
welcome.workshop.package_delivered
welcome.caravan.important.expected
welcome.caravan.important.late
```

Prefer facts that describe something true or known rather than names like `quest_4_stage_3`.

A situation may be referenced by several NPCs. It is not owned by whichever NPC first exposes it to the player.

This preserves the Welcome requirement that the workshop package can be discovered from either the workshop or the transit host with different dialogue framing.

## Shared world vs character-relative content

N0 records the design requirement but does not implement it.

Future runtime work must distinguish at least:

- shared authoritative NPC/world state;
- character-relative narrative facts;
- client-local conversational presentation where safe.

Two players may interact with the same physical NPC while receiving different eligible dialogue according to their own progression.

A private dialogue gesture must not silently mutate the shared physical NPC state.

## NPC knowledge

N0 does not assume that every NPC automatically knows every player/world fact.

The authored model must remain compatible with later distinction between:

- what is true in the world;
- what the player knows;
- what a particular NPC can know or infer.

Do not build a generic knowledge simulation in N0.

## Validation expectations

Authoring validation must eventually reject at minimum:

- unsupported `schema_version`;
- missing/invalid NPC `ContentId`;
- duplicate NPC IDs;
- duplicate local beat IDs inside one NPC;
- broken `next` beat references;
- malformed condition records;
- invalid logical asset references when the relevant asset registry exists;
- unresolved strict NPC references once those references are marked runtime-relevant.

The current `purgatory-content-validator` is not changed in N0 because `content/authoring/npcs/` is intentionally not part of the live runtime pack yet.

N1+ may add a dedicated NPC authoring validator or extend the existing validator only when the ownership is clear.

## Runtime projection rule

The canonical authoring JSON must not be interpreted directly on simulation hot paths.

When runtime integration begins, the expected direction is:

```text
authoring JSON
-> validate
-> resolve authored references
-> typed Rust definitions / registries
-> authoritative evaluator / domain-owner requests
```

Server code should not need voice files, animations, or other client-only presentation assets to evaluate gameplay truth.

The exact physical projection split is intentionally deferred.

## Local tooling rule

NPC Lab will live locally under `tools/npc_lab/` and should eventually be launchable through the existing Developer Hub/tooling path.

The tool edits repository files directly or through a local bridge. It must not require cloud storage or a hosted backend.

## Explicit N0 non-goals

N0 does not implement:

- NPC runtime spawning;
- dialogue UI in the game client;
- dialogue protocol/network messages;
- per-character narrative storage;
- persistence;
- quest manager;
- gameplay scripting;
- JavaScript/Lua embedding;
- behavior trees;
- NPC schedules;
- shop runtime;
- voice generation/playback;
- localization;
- lip sync;
- animation playback;
- runtime content registry changes;
- final JSON schema for all future NPC features.

## N0 proof NPC

The first proof document is:

```text
content/authoring/npcs/welcome/npc.welcome.traveler_stayed.json
```

It represents the Welcome transit-host NPC currently known as "the traveler who stayed".

The proof must show that the current design can be recorded with:

- stable NPC identity;
- character/design context;
- cross-NPC relationship placeholders;
- multiple dialogue beats;
- player choices;
- declarative conditions;
- optional lore/filler classification;
- presentation references reserved without implementing them.

If several real NPCs later expose a poor fit, change the authoring contract deliberately rather than preserving N0 for compatibility aesthetics.

## N0 gate

N0 is ready for review when:

1. the contract exists;
2. the authoring/tool directories exist in Git;
3. NPC #3 has a compact valid JSON proof document;
4. no runtime/PHASE/protocol behavior changed;
5. the document is understandable without requiring JavaScript or a custom DSL.
