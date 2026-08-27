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

Human-editable JSON lives under `/content/definitions`:

- `monsters/`
- `items/`
- `skills/`
- `maps/`

Development-only fixtures may live under `/content/dev`.

Presentation assets will later live under `/assets`. `/assets/dev` is reserved for placeholder/dev assets.

The existing `Graphic/` directory is not part of this pipeline yet. Do not import, scan, or connect it to the runtime.

## IDs

Author IDs are stable strings, for example:

```text
monster.slime.green
item.consumable.small_potion
skill.basic.strike
```

At load time the runtime may assign compact internal IDs. Simulation uses definitions, not authoring files, on the hot path.

## Validation

`purgatory-content-validator` must eventually:

- scan definitions
- report duplicate IDs
- report missing required fields
- report broken references
- exit non-zero on invalid content

Invalid content must be detected before players see it.

## Runtime rules

- The server must not load image textures to understand a definition.
- Adding a normal monster, item, or skill is data plus optional presentation data.
- Engine source changes are required only when content introduces genuinely new behavior.
- Simple stat changes must not require rebuilding Rust once live/dev reload exists and is safe.

## Questions every content system must answer

1. What part is data?
2. What part is reusable behavior?
3. What part is authoritative simulation?
4. What part is client presentation?
5. Can a content author create a normal variant without editing core source?
6. Can invalid content be detected before players see it?
