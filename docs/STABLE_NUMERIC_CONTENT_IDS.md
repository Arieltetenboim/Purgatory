# Stable Numeric Content IDs

Status: **accepted owner architecture principle**

This document is authoritative for future content identity work. It supersedes the **canonical authored-string identity** portion of ADR-0036 and any later documentation that treats a string such as `item.foo` or `npc.area.name` as the permanent identity of content.

The current implementation has **not yet been migrated**. Existing string-backed `ContentId` behavior remains a temporary compatibility state until a dedicated migration is performed.

## Principle

Every globally unique authored game-content definition has one stable numeric ID.

The number is the canonical identity. Human-readable names, working names, display names, filenames, tags and search labels are metadata and may change without changing identity.

Once an ID has been allocated to content, its meaning is permanent:

- never renumber released/persisted content merely to reorganize the catalog;
- never reuse an ID that belonged to deleted or retired content;
- retired IDs stay retired;
- references, persistence, networking, admin commands and tools must ultimately converge on the same stable number.

Runtime-instance identities remain separate. A spawned monster/NPC/world drop still has a runtime `EntityId`; an owned economic item still has an `ItemInstanceId`. The numeric content ID identifies the **definition/template**, not one spawned instance.

## Global 10,000-ID domain blocks

The ID space is globally unique and partitioned into explicit 10,000-ID content-domain blocks.

Frozen initial blocks:

| Numeric range | Domain |
| ---: | --- |
| `10,000–19,999` | Monsters |
| `20,000–29,999` | NPCs |
| `30,000–39,999` | Items |
| `40,000–49,999` | Skills / Abilities |

`0–9,999` is currently unallocated/reserved. Additional 10,000-ID blocks must be assigned explicitly when another durable content domain needs one; do not silently invent allocations.

The block identifies the broad content domain only. Do **not** subdivide a block into permanent semantic ranges such as weapons vs armor vs consumables unless a later explicit architecture decision requires it. Item/monster/NPC subtype belongs in the definition data, not in hidden numeric arithmetic.

Examples (illustrative numbers, not allocations unless separately entered into the catalog):

```text
10001 -> a Monster definition
20001 -> an NPC definition
30001 -> an Item definition
40001 -> a Skill/Ability definition
```

A command such as:

```text
/spawnitem 30001
```

must eventually resolve `30001` directly through the authoritative Item registry and spawn an instance/world drop of that definition. Passing an ID from another domain to an item command must fail validation rather than reinterpret the number.

## Type safety in Rust

Global uniqueness does not prevent using domain wrappers in code. Wrappers are encouraged where they prevent accidental API misuse:

```rust
struct ContentId(u32);
struct ItemId(ContentId);
struct NpcId(ContentId);
struct MonsterId(ContentId);
struct AbilityId(ContentId);
```

The raw numeric value remains globally unique. Typed wrappers are compile-time/API safety, not separate numeric namespaces.

A generic registry boundary may accept `ContentId`; a domain owner such as item spawning may accept `ItemId` or validate that a `ContentId` belongs to the Item block.

## Authoring and tools

Authoring tools may display rich labels next to IDs, for example:

```text
30017 — Practice Sword
```

but must not create a second canonical string identity. Search labels and filenames are conveniences only.

References between durable content definitions should ultimately store the numeric ID. Examples after migration:

```json
{ "npc_met": 20002, "equals": true }
```

```json
{ "give_item": { "item": 30002, "quantity": 1 } }
```

NPC Lab and other FORGE tools may continue using the temporary current string schema until the shared content-ID migration is deliberately performed; new tooling should not deepen dependence on string identity.

## Composite/facet definitions

Multiple files/facets that describe the **same logical content object** should share its content ID rather than consume unrelated IDs merely because they are stored separately.

Example: an equippable sword's item definition, equipment rules and equipment-presentation facet are all facets of the same Item and should resolve to the same Item-range content ID.

Local IDs inside one definition (for example a dialogue beat ID or an attachment-local ID) are not automatically global content IDs. Promote a concept to the global ID catalog only when it needs durable cross-definition/runtime identity.

## Migration boundary

The existing code currently defines `ContentId` as a `u64` FNV-1a hash of a canonical authored string and uses strings as registry keys. Existing protocol paths serialize that 64-bit token. This is now legacy behavior to be replaced, not the target architecture.

A migration must deliberately cover the shared identity boundary rather than changing one content type in isolation:

1. replace canonical string/hash `ContentId` construction with an explicit stable numeric value (expected storage: `u32`);
2. add block/domain validation and global duplicate-ID validation;
3. change content JSON definition IDs and durable cross-content references from canonical strings to numeric IDs;
4. make registries primarily ID-keyed; retain names/labels only as metadata/search indexes where useful;
5. migrate live item/equipment/ability/map/entity references and tests;
6. update protocol codecs and bump protocol version if the wire representation changes from the current 8-byte token;
7. migrate FORGE authoring schemas, including NPC references, after the shared identity primitive is stable;
8. before persistence ships, define the catalog/retirement process that prevents accidental ID reuse.

Do not perform this as opportunistic cleanup inside an unrelated Phase/FORGE slice.

## Non-goals of this decision

This decision does not yet allocate IDs to existing content, assign blocks beyond the frozen initial set, define a database sequence, define public/mod IDs, or decide how externally authored/modded content would coexist with first-party IDs.
