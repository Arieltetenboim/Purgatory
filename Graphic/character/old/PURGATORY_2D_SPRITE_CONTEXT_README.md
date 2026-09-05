# PURGATORY 2D — Player Sprite and Paper Doll Context

This file is the authoritative context for any AI system creating, editing, upscaling, slicing, or validating the player sprite and its Paper Doll layers.

## 1. Non-negotiable rules

- Never place Hebrew text inside the game or inside any game asset unless the user explicitly requests it.
- The sprite must be an original design. It may use the broad visual principles of cute classic 2D side-scrollers, but it must not copy a protected character, costume, map, logo, or exact sprite.
- Keep the character compact and highly readable: very large rounded head, tiny body, short limbs, clean silhouette, simple shading, and restrained detail.
- Match the established PURGATORY 2D environment assets: colorful modern fantasy rendering, clean dark outlines, soft restrained shading, and good readability at small display sizes.
- Do not add stars, dust, impact marks, motion lines, glows, particles, shadows, labels, grid lines, or decorative debris to the base sprite or Paper Doll overlays.
- Transparency must be genuine PNG alpha. Never bake a checkerboard pattern into an image.
- Do not crop individual frames tightly. Every layer must retain the exact same canvas, cells, registration, and anchor positions.
- Do not invent additional animations or fill intentionally unused cells.

## 2. Master atlas contract

- Layout: 6 columns × 19 rows.
- Total capacity: 114 fixed cells.
- Every row represents one animation.
- Animations may use fewer than six frames. Unused cells must remain fully transparent.
- All atlases must have identical pixel dimensions.
- Every cell must have identical dimensions.
- The primary ground anchor is the center point between the feet on the cell baseline.
- Airborne and climbing poses retain the same logical character origin even when the feet do not touch the baseline.
- Right-facing versions are created at runtime by horizontal mirroring. Do not create duplicate right-facing rows.

## 3. Animation rows

| Row | Animation ID | Facing | Frames | Required motion |
|---:|---|---|---:|---|
| 1 | `IDLE_FRONT` | Toward viewer | 6 | Simple frontal idle. Subtle breathing is allowed. Eye layer performs a blink. |
| 2 | `WALK_SIDE` | Left | 6 | Compact readable walk loop. No separate run animation. |
| 3 | `CROUCH_SIDE` | Left | 1 | Very low crouch / near-prone pose. |
| 4 | `JUMP_SIDE` | Left | 1 | One airborne pose used from takeoff until ground contact. No fall or landing animation. |
| 5 | `HIT_SIDE` | Left | 2 | Very short damage reaction. No recovery animation. |
| 6 | `KNOCKBACK_SIDE` | Left | 1 | Body recoils backward in a small airborne pose. Movement and flashing are engine effects. |
| 7 | `DEATH_GRAVE` | Front | 1 | A simple grave replaces the body. |
| 8 | `ATTACK_SWORD` | Left | 4 | One-handed sword swing. Body and hands only in the body atlas; sword is a separate equipment layer. |
| 9 | `ATTACK_BOW` | Left | 4 | Raise, draw, release, settle. Bow and arrow are separate layers/assets. |
| 10 | `CAST_ATTACK` | Left | 3 | Offensive spell gesture. Spell effect is separate. |
| 11 | `CAST_BLESS` | Front | 3 | Blessing/support gesture. Spell effect is separate. |
| 12 | `CAST_CHANNEL` | Front | 3 | Waiting/channeling spell pose. Spell effect is separate. |
| 13 | `ATTACK_SHURIKEN` | Left | 3 | Prepare, throw, finish. Projectile is separate. |
| 14 | `CLIMB_LADDER` | Back toward viewer | 4 | Alternating hands and feet, seamless loop. |
| 15 | `CLIMB_ROPE` | Front | 4 | Alternating grip and legs, seamless loop. |
| 16 | `INTERACT` | Left | 2 | Neutral hand reach / press interaction. |
| 17 | `PICK_UP` | Left | 2 | Lower body and lift an object. Object is separate. |
| 18 | `WAVE_FRONT` | Toward viewer | 4 | Friendly wave to another player. |
| 19 | `POINT_SIDE` | Left | 2 | Raise arm and point forward. Mirror at runtime to point right. |

## 4. Timing and transition philosophy

- Animation is intentionally simple and may feel slightly snappy.
- Gameplay input selects the next state directly.
- Jump uses one persistent airborne frame. On ground contact, switch directly to idle or walk according to current input.
- Hit has no recovery frames.
- Knockback displacement, invulnerability flashing, and hit timing are controlled by the game engine, not drawn into the sprite.
- Death swaps the player for the grave frame.
- Projectile travel and magic effects are separate gameplay assets.

## 5. Paper Doll layer contract

Create each category as a separate full-size transparent atlas. Never combine optional categories into one image.

Recommended draw order from back to front:

1. Back equipment / cape
2. Base body
3. Pants / lower-body clothing
4. Shirt / armor
5. Shoes
6. Eyes / facial features
7. Hair
8. Gloves / hand equipment
9. Held weapon
10. Front equipment
11. Runtime effects

Minimum required atlases:

- `player_body_base.png`
- `player_eyes.png`
- `player_hair_short.png`
- `player_pants_basic.png`

Each optional item must be independently usable. For example, eyes, hair, and pants must never be baked into a single overlay.

## 6. Eye-layer requirements

- Eyes must be small, simple, and suited to the head angle.
- Side-facing poses normally show one visible eye.
- Front-facing poses show two eyes.
- Back-facing ladder poses show no eyes.
- Row 1 blink sequence: open, open, half-closed, fully closed, half-closed, open.
- Blinking must not move the eye anchor.
- Do not use large oval anime eyes unless a later explicit design decision requests them.

## 7. Hair-layer requirements

- Short, simple, compact hairstyle.
- One consistent design across every pose and head orientation.
- Adapt correctly to frontal, side, tilted, and rear views.
- Do not add hats, ribbons, clips, long strands, or other accessories.

## 8. Pants-layer requirements

- Simple basic pants with restrained detail.
- Pants must follow pelvis and upper-leg deformation in every pose.
- Keep feet and shoes separate.
- Do not include belts, pouches, straps, skin, legs, feet, or torso pixels.

## 9. Weapon and effect separation

- Never bake weapons into the base body.
- The body atlas supplies the correct arm and hand poses for each weapon family.
- Sword, bow, arrow, shuriken, grave, projectiles, spell visuals, hit flashes, and particles are separate assets or layers.
- Runtime mirroring must mirror the character and all equipped layers together.

## 10. Upscaling and export

- Lock the complete body atlas before producing final Paper Doll equipment.
- Keep one high-resolution master atlas and derive smaller runtime exports from it.
- Apply exactly the same resize operation to every layer.
- Never AI-upscale Paper Doll layers independently; independent reinterpretation changes edges and breaks registration.
- If AI upscaling is required, first composite all layers for visual reference, upscale a locked master consistently, then transfer the same geometric transform to every source layer and validate pixel-perfect overlay alignment.
- Preserve alpha edges. Do not add background color, halos, sharpening artifacts, or opaque fringe pixels.

## 11. Validation checklist

Before accepting an atlas, verify:

- Correct 6 × 19 layout.
- Correct frame count per row.
- Row 1 faces the viewer.
- All side-facing rows use the same left direction.
- No unintended right-facing duplicates.
- No body drift or scale changes between frames.
- Stable ground anchor.
- Genuine transparent background.
- No checkerboard baked into the image.
- No text of any language.
- No particles or decorative effects.
- Every Paper Doll layer overlays the body without scaling or manual repositioning.
- Empty cells are fully transparent.

