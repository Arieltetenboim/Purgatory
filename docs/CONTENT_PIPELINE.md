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
- `shared/equipment/` — gameplay equipment identity (`schema_version`, `id`, `equipment_slot`)
- `shared/equipment_presentation/` — client presentation for the same `ContentId` (`attachments[]`)
- `shared/abilities/` — gameplay ability JSON (`AbilityDefinition`; schema_version 1). Loaded in Shared and Full modes.
- `shared/animations/dev/` — A6/A7 v1 `.anim` presentation clips (token text, not JSON). Optional `depth` keys (A7.1 `depth_angle`; omitted = 0). Authored in Animation Lab. Runtime still compiles them in via `include_str!`.
- `server/entities/` — server-only entities (interactables, portals with `transition: { map, portal }`)
- `server/placements/` — server-only placement lists keyed by map authored id

JSON must not contain numeric `MapId`, channel, or instance. The registry assigns `MapId` (FOOTNOTE / `map.dev.footnote` is pinned to `MapId` 1).

Each map authors a **restore** policy (`safe_point`, `checkpoint`, or `non_reenterable`). That is restore semantics, not a `WorldAddress`. Channel and runtime Instance are assigned by a separate placement layer (Phase 6E currently uses DEFAULT). See ADR-0044.

Development-only hard-coded fixtures (`World::footnote_test_stage`) remain for unit tests. Live server/client paths load this pack.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

The existing `Graphic/` directory is not part of this pipeline yet. Phase 5.0B loads **only** `Graphic/LOGO.png` for the Connection Frontend (temporary filesystem path). Developer Hub can export `Graphic/character/HUMANOID_V0_AUTHORING_TEMPLATE.svg` as a Photoshop reference from Humanoid v0 contracts, plus an empty Headwear Side master overlay at `Graphic/character/headwear_side/HEADWEAR_SIDE_MASTER_V1.svg`. Animation Lab may load one DEV-only extracted Headwear Side PNG (`equipment.debug.headwear_proof.a.side`) for a compose proof. The game client compile-embeds the four extracted Side cells (`a`–`d`) via `include_bytes!` onto the existing Headwear Crown attachment; Player debug overlay selects cells 1–4. It must not scan or filesystem-load Graphic/ character files, and this is not an asset loader. Do not import, scan, or otherwise connect the rest of `Graphic/` to the runtime.

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
