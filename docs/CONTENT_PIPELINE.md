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
- `shared/items/` — generic item gameplay definitions (schema v3: numeric canonical `id`, metadata `label`, `category`, `stack_limit`)
- `shared/item_presentation/` — optional client-safe item icon selection (schema v2) for the same numeric Item `ContentId`
- `shared/equipment/` — gameplay equipment facet (schema v2: same numeric Item `id` + metadata `label` + `equipment_slot`)
- `shared/equipment_presentation/` — client presentation facet (schema v2) for the same numeric Item `ContentId` (`attachments[]`)
- `shared/abilities/` — gameplay ability JSON (`AbilityDefinition`; schema_version 1). Loaded in Shared and Full modes.
- `shared/animations/dev/` — A6/A7 v1 `.anim` presentation clips (token text, not JSON). Optional `depth` keys (A7.1 `depth_angle`; omitted = 0). Authored in Animation Lab. Runtime still compiles them in via `include_str!`.
- `authoring/maps/` — PURGATORY map-authoring sources. The map sidecar owns
  identity, the relative Tiled TMX visual source, and explicit per-map PPU.
  Ordinary production maps use the locked **100 px/wu** visual-scale standard; changing
  camera zoom must not be modeled by changing PPU. `purgatory-content` compiles TMX/TSX through one shared compiler into
  versioned canonical `MapPresentation`; Map Lab previews that same output.
  TMX remains the **static visual-composition source** and is not runtime input.
  Maps are registry-driven content. Authored maps are discovered and compiled from content definitions. The client resolves presentation by ContentId through ContentRegistry. Adding a map requires no Rust code change.
  TMX/TSX are authoring-only compiler inputs and are not runtime map identity.
  Map Lab owns gameplay/world authoring that is not visual composition: FOOTNOTE paths,
  spawn points, NPC placement and Mob placement. Map Lab also owns map-level dynamic
  environment presentation: sky gradients, semantic parallax/depth layers, celestial
  sprites, and moving/wrapping cloud layers. Environment depth is authored as presentation
  depth/parallax semantics rather than fake physical distance. Tiled continues to own the
  underlying static artwork and object composition; Map Lab owns how environment layers
  behave relative to the camera. Dynamic/moving platform gameplay remains deferred, but
  authored FOOTNOTE geometry must not preclude future entity-owned moving foothold groups.
  New maps are created in Map Lab, which allocates the next unused map ContentId from
  `content/CONTENT_ID_CATALOG.md` (retired IDs stay retired), writes
  `Graphic/assets/maps/<ContentId>.tmx`, and writes the sidecar, gameplay, environment,
  and placement files. The numeric TMX filename follows that ContentId. Importing an
  existing numeric TMX remains available for exceptional cases. Reloading a TMX updates
  the visual compile only; gameplay, environment, and placements stay as authored, including
  when a later bounds change leaves them outside the map.
- `authoring/npcs/` — canonical NPC Lab JSON. Recursively validated in Shared
  and Full modes; projected into client-safe dialogue presentation in both and
  authoritative dialogue definitions in Full mode.
- `server/entities/` — server-only reusable entity definitions (NPC/entity/interactable facets). Legacy `entity.portal.*` definitions with `transition: { map, portal }` remain read-compatible, but Map Lab does not create new portals through this path.
- `server/placements/` — server-only placement lists keyed by map authored id. Placement schema v2 gives ordinary entity/monster placements a stable map-local `id`, a closed `kind`, a `content` authored reference, and a world `position`. Portals are map-owned placements: `kind: "portal"`, a map-local Portal ID such as `portal.001`, a world position, and optional `linked_portal: { map, portal }`. The linked target is identified explicitly by destination MapID plus destination Portal ID. Schema v1 `{ entity, position }` remains load-compatible and receives deterministic legacy compatibility IDs; new Map Lab writes v2.
- `definitions/monsters/` — canonical Monster schema v4 definitions. Full mode loads authoritative gameplay fields; Shared mode projects only client-safe Monster presentation identity. Mob Lab edits these files directly.

JSON does not contain runtime `MapId`, channel, or instance. Stable map identity is the numeric `ContentId`; the registry derives runtime `MapId` from the map ContentId block.

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
identity. Monster definitions remain owned by Mob Lab. Entity E2 projects both Map Lab monster placements and DEV monster spawns through the same content-backed runtime adapter, so HP, collision, movement speed, approach bounds and home leash have one owner. A Map Lab monster placement position is the monster's floor/contact point; the adapter derives the runtime entity origin from authored collision bounds.

Portal links are map-placement data. New portals are authored in Map Lab as map-owned placements with a map-local Portal ID and `linked_portal: { "map": "<destination MapID>", "portal": "<destination Portal ID>" }`. The editor displays targets as `MapID · PortalID`; choosing a portal therefore carries both pieces of identity explicitly. Arrival is at that linked portal, not the map's generic spawn. Links are one-way unless the destination portal also links back. Legacy portal entity `transition` metadata remains read-compatible during migration but is not the forward authoring workflow.

Phase **8A** stores equipped appearance as `EquipmentSlot → Option<ContentId>` using the existing `ContentId` type. Phase **8B** introduced the gameplay slot + client presentation split; the current Item/Equipment schemas use the stable numeric Item catalog described below. Phase **8C** authorizes Equip/Unequip from gameplay definitions only (`authorize_equip`); presentation is not required on the server path. Presentation fields (bones, anchors, visuals, coverage) are not on the network. Phase **8D** carries replicated equipment on `CharacterPresentationState`. Phase **8E** resolves those ContentIds to bound attachments on the client (`equipment_presentation_by_id`) and composes debug placeholders; it does not load ART.

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

## Item / Equipment stable numeric identity

Gameplay equipment JSON (`content/shared/equipment/*.json`) uses schema v2:

```text
{ "schema_version": 2, "id": 30001, "label": "equipment.debug.cloth_cap", "equipment_slot": "headwear|bodywear|pants|gloves|boots|weapon" }
```

Item gameplay JSON (`content/shared/items/*.json`) uses schema v3:

```text
{
  "schema_version": 3,
  "id": 30001,
  "label": "equipment.debug.cloth_cap",
  "category": "equipment|consumable|material|tool|misc",
  "stack_limit": 1
}
```

The numeric `id` is canonical and must be an allocated Item-block ID (`30,000–39,999`) from `content/CONTENT_ID_CATALOG.md`. `label` is human-readable authoring/search metadata and must match the catalog allocation exactly.

Every Equipment definition must have an Item definition with the same canonical `ContentId`.
Equipment-backed items must use the `equipment` category, and an `equipment`
category item must have a matching Equipment definition. Category is gameplay
data because future inventory capacity policy may depend on it; it is not
inferred from an icon or filename.

Optional item presentation JSON (`content/shared/item_presentation/*.json`) uses schema v2 and repeats the same numeric `id` plus `label`.
uses the same `id`:

```text
{ "schema_version": 1, "id": "<ContentId>", "icon": "<logical visual key>" }
```

`icon` is not a filesystem path or GPU handle. The client resolves the logical
key through its presentation asset layer. Missing item presentation, or a
presentation key unavailable in the current client build, uses the explicit
inventory placeholder icon.

Presentation JSON (`content/shared/equipment_presentation/*.json`) uses schema v2 and the **same numeric Item `id` + `label`**. It must not repeat `equipment_slot`. Attachments are `0..N`:

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
