# Content pipeline

Target authoring workflow:

```text
author data
→ validate
→ load into runtime registry
→ map author-facing string ID to runtime ID
→ simulation uses definition
→ client maps presentation references to assets
```

## Authoring location

Human-editable JSON lives under `/content`:

- `shared/maps/` — client-safe map geometry, bounds, spawn points
- `shared/entities/` — client-safe entity definitions (none required for 6C)
- `shared/items/` — generic item gameplay definitions (`schema_version`, `id`, `category`, `stack_limit`)
- `shared/item_presentation/` — optional client-safe item icon selection for the same `ContentId`
- `shared/equipment/` — gameplay equipment identity (`schema_version`, `id`, `equipment_slot`)
- `shared/equipment_presentation/` — client presentation for the same `ContentId` (`attachments[]`)
- `shared/abilities/` — gameplay ability JSON (`AbilityDefinition`; schema_version 1). Loaded in Shared and Full modes.
- `shared/animations/dev/` — A6/A7 v1 `.anim` presentation clips (token text, not JSON). Optional `depth` keys (A7.1 `depth_angle`; omitted = 0). Authored in Animation Lab. Runtime still compiles them in via `include_str!`.
- `authoring/npcs/` — canonical NPC Lab JSON. Recursively validated in Shared
  and Full modes; projected into client-safe dialogue presentation in both and
  authoritative dialogue definitions in Full mode.
- `server/entities/` — server-only entities (interactables, portals with `transition: { map, portal }`)
- `server/placements/` — server-only placement lists keyed by map authored id
- `definitions/monsters/` — canonical Monster schema v4 definitions. Full mode loads authoritative gameplay fields; Shared mode projects only client-safe Monster presentation identity. Mob Lab edits these files directly.

JSON must not contain numeric `MapId`, channel, or instance. The registry assigns `MapId` (FOOTNOTE / `map.dev.footnote` is pinned to `MapId` 1).

Each map authors a **restore** policy (`safe_point`, `checkpoint`, or `non_reenterable`). That is restore semantics, not a `WorldAddress`. Channel and runtime Instance are assigned by a separate placement layer (Phase 6E currently uses DEFAULT). See ADR-0044.

Development-only hard-coded fixtures (`World::footnote_test_stage`) remain for unit tests. Live server/client paths load this pack.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

`Graphic/` is not a general runtime scan. Specific presentation paths are integrated deliberately: the client uses `Graphic/frontend/LOGO.png` for the Connection Frontend, UI atlas assets for production windows, and creature sprite manifests/atlases under `Graphic/creature/*` through the validated Monster presentation path. Character Lab authors the Humanoid v0 / Template V1 workflow and the current `Graphic/character/base/character.base.dev_01.visual-pack.json` + side atlas. The client compile-embeds that validated visual-pack manifest and atlas through `character_assets.rs`; this is an explicit integration, not a directory scan or runtime filesystem loader. Developer Hub also maintains the Humanoid v0 authoring/template exports and the Headwear Side proof path. Animation Lab may load one DEV-only extracted Headwear Side PNG for compose proof, and the client compile-embeds the four proof cells for debug selection. Do not infer from these explicit paths that arbitrary `Graphic/` files are runtime content.

## IDs

Author IDs are stable strings, for example:

```text
monster.slime.green
item.consumable.small_potion
skill.basic.strike
```

At load time the runtime may assign compact internal IDs. Simulation uses definitions, not authoring files, on the hot path.

## Validation

`purgatory-content-validator` loads the workspace pack (`LoadMode::Full`) and:

- scans definitions
- reports duplicate IDs
- reports missing required fields
- reports broken references
- exits non-zero on invalid content

The quality gate runs it after `cargo test`. Invalid content must be detected before players see it.

## Runtime rules

- The server must not load image textures to understand a definition.
- Adding a normal monster, item, or skill is data plus optional presentation data.
- Engine source changes are required only when content introduces genuinely new behavior.
- Simple stat changes must not require rebuilding Rust once live/dev reload exists and is safe.

The Social NPC dialogue runtime is content-backed. Canonical
`authoring/npcs/**/*.json` documents are validated and projected into typed
`NpcDialogueDefinition` and `NpcDialoguePresentation` registries keyed by the
same numeric NPC `ContentId` as the matching server entity facet. Shared mode
exposes only client-safe Beat text, choice labels and animation cues; Full mode
also exposes authoritative conditions, pools, continuations and actions. See
[`NPC_DIALOGUE_RUNTIME.md`](NPC_DIALOGUE_RUNTIME.md).

The Monster schema v4 contract is
[`MONSTER_AUTHORING_RUNTIME.md`](MONSTER_AUTHORING_RUNTIME.md). The Red Slime
normal-session proof resolves Health, collision half-extents, movement speed, damage-triggered chase behavior and home leash from the validated Full registry. Workload-only NPC
presets and tokens remain owned by simulation/server code and are not monster
identity. Monster placement, presentation, abilities and loot are not part of
schema v1.

Portal links are content data: `transition: { "map": "<dest map authored id>", "portal": "<dest portal entity authored id>" }`. Arrival is at the linked portal, not the map's generic spawn. The destination entity does not need a reverse `transition` (one-way portals).

Phase **8A** stores equipped appearance as `EquipmentSlot → Option<ContentId>` using the existing `ContentId` type. Phase **8B** validates Equipment Content Schema v1 (gameplay slot + client presentation). Phase **8C** authorizes Equip/Unequip from gameplay definitions only (`authorize_equip`); presentation is not required on the server path. Presentation fields (bones, anchors, visuals, coverage) are not on the network. Phase **8D** carries replicated equipment on `CharacterPresentationState`. Phase **8E** resolves those ContentIds to bound attachments on the client (`equipment_presentation_by_id`) and composes debug placeholders; it does not load ART.

## Ability definition (Phase 9A / 9B)

Runtime contract lives in `purgatory-simulation` (`AbilityId` = `ContentId`). Pack files live in `content/shared/abilities/`. v1 example (`skill.basic.strike`):

```text
{
  "schema_version": 1,
  "id": "skill.basic.strike",
  "timing": { "windup_ticks": 3, "active_ticks": 2, "recovery_ticks": 4, "cooldown_ticks": 12 },
  "activation": "independent",
  "delivery": { "kind": "forward_query", "range": 1.5, "half_height": 0.8, "max_targets": 8 },
  "effects": [{ "type": "damage", "amount": 5.0 }]
}
```

- `id` uses the existing authored-id rules.
- Timing is simulation ticks (30 Hz). Zero skips that live Action phase.
- `activation` is who is required to **start** (`independent` | `selected_entity`). It is not the hit list.
- `delivery` is who is affected at Active (`forward_query` | `selected_entity`). Zero hits is valid.
- `effects[]` is an ordered closed enum. v1 is `damage` only. Do not add heal/buff fields onto the definition struct.
- Engine source changes are required only for new effect kinds or delivery/activation kinds.
- Basic Attack is this data, not hardcoded combat constants. 7.2 `STRIKE_*` constants are workload placeholders.
- Phase **9C** authorization is a World `AbilityGrantTable`, not a content skill-book. Pack membership is not a grant.

## Equipment Content Schema v1

Gameplay JSON (`content/shared/equipment/*.json`):

```text
{ "schema_version": 1, "id": "<ContentId>", "equipment_slot": "headwear|bodywear|pants|gloves|boots|weapon" }
```

Item Gameplay Schema v2 (`content/shared/items/*.json`):

```text
{
  "schema_version": 2,
  "id": "<ContentId>",
  "category": "equipment|consumable|material|tool|misc",
  "stack_limit": 1
}
```

Every Equipment definition must have an Item definition with the same canonical `ContentId`.
Equipment-backed items must use the `equipment` category, and an `equipment`
category item must have a matching Equipment definition. Category is gameplay
data because future inventory capacity policy may depend on it; it is not
inferred from an icon or filename.

Optional item presentation JSON (`content/shared/item_presentation/*.json`)
uses the same `id`:

```text
{ "schema_version": 1, "id": "<ContentId>", "icon": "<logical visual key>" }
```

`icon` is not a filesystem path or GPU handle. The client resolves the logical
key through its presentation asset layer. Missing item presentation, or a
presentation key unavailable in the current client build, uses the explicit
inventory placeholder icon.

Presentation JSON (`content/shared/equipment_presentation/*.json`) uses the **same** `id`. It must not repeat `equipment_slot`. Attachments are `0..N`:

```text
id, bone, anchor, coverage, hide_base?, correction?, visuals.side, visuals.back?
```

- `bone` / `anchor` are closed enums (`BoneTarget` / `AnchorPoint`). No free-form names.
- `coverage` is `overlay` (empty `hide_base`) or `replace_base` (may list base-body `BoneTarget`s only).
- `visuals.side` is a logical visual key, not a PNG path. `visuals.back` is optional.
- Character Presentation (8F-E) selects the key from `PresentationView` (`Side` → `visuals.side`, `Back` → `visuals.back`). Missing Back is omitted from the Back draw plan; Side is never substituted.
- `correction` is local `{x,y,rotation}` with bounds ±8 px / ±15 deg. No scale.
- Invalid content fails the pack load. There is no silent clamp, substitute, or Side-as-Back fallback.

`PresentationCompleteness { has_side, has_back }` is the 8F-E selection hook. 8B does not require Back by slot. Side-only pack items (gloves, sword) remain valid.

## Questions every content system must answer

1. What part is data?
2. What part is reusable behavior?
3. What part is authoritative simulation?
4. What part is client presentation?
5. Can a content author create a normal variant without editing core source?
6. Can invalid content be detected before players see it?


### Monster sprite selection

Monster definitions own a stable `sprite` id. Client-safe loading projects that
presentation identity from the same `content/definitions/monsters/*.json` source,
while full server loading additionally materializes gameplay fields. Creature sprite
manifests live under `Graphic/creature/*/manifest.json` and are consumed directly
by Mob Lab and the client presentation loader.
