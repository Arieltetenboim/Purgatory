# Player Sprite Generation Prompt

Use the attached body/reference sprite only as a registration, proportion, and visual-language reference. Create a new original production-ready master player sprite atlas for PURGATORY 2D.

## Primary request

Create a clean modular Paper Doll base-body atlas for a cute 2D side-scrolling game. The character must have a very large rounded head, tiny compact body, short limbs, strong readable silhouette, clean dark outlines, restrained soft shading, and a modern fantasy finish compatible with the existing PURGATORY 2D environment assets.

The base body must be neutral and modest, with no hair, detailed eyes, pants, shirt, shoes, armor, weapon, equipment, accessories, projectiles, magic, particles, or decorative effects. It must be suitable for placing independent transparent Paper Doll layers over it.

## Exact atlas layout

- 6 columns × 19 rows.
- Fixed identical cell dimensions.
- Preserve one stable character scale and one stable logical anchor across the entire atlas.
- Unused cells in a row must remain fully transparent.
- Do not duplicate right-facing animations; the game engine mirrors left-facing frames at runtime.

## Required rows

1. `IDLE_FRONT` — 6 frames — character faces directly toward the viewer; very subtle idle movement.
2. `WALK_SIDE` — 6 frames — left-facing compact walk loop.
3. `CROUCH_SIDE` — 1 frame — very low crouch / near-prone pose.
4. `JUMP_SIDE` — 1 frame — left-facing airborne pose used for the complete jump.
5. `HIT_SIDE` — 2 frames — short left-facing damage reaction; no recovery.
6. `KNOCKBACK_SIDE` — 1 frame — left-facing backward airborne recoil.
7. `DEATH_GRAVE` — 1 frame — simple grave replacement; no character body.
8. `ATTACK_SWORD` — 4 frames — left-facing one-handed sword body/hand motion; do not draw the sword.
9. `ATTACK_BOW` — 4 frames — left-facing raise/draw/release motion; do not draw bow or arrow.
10. `CAST_ATTACK` — 3 frames — left-facing offensive casting gesture; no magic effect.
11. `CAST_BLESS` — 3 frames — frontal blessing/support gesture; no magic effect.
12. `CAST_CHANNEL` — 3 frames — frontal waiting/channeling gesture; no magic effect.
13. `ATTACK_SHURIKEN` — 3 frames — left-facing throwing motion; do not draw a shuriken.
14. `CLIMB_LADDER` — 4 frames — rear-facing alternating climb loop; do not draw a ladder.
15. `CLIMB_ROPE` — 4 frames — frontal alternating climb loop; do not draw a rope.
16. `INTERACT` — 2 frames — left-facing reach/press motion.
17. `PICK_UP` — 2 frames — left-facing lower-and-lift motion; do not draw an object.
18. `WAVE_FRONT` — 4 frames — frontal friendly wave.
19. `POINT_SIDE` — 2 frames — left-facing point-forward gesture.

## Style constraints

- Original character design; do not reproduce an existing commercial character or exact sprite.
- Cute classic side-scroller proportions with a modern, slightly polished rendering.
- Simplify anatomy and surface detail.
- Keep the character readable at small in-game size.
- Keep body proportions, head shape, outline weight, colors, lighting direction, and shading consistent across all frames.
- Snappy minimal animation is intentional. Do not add transition, recovery, falling, landing, or extra idle animations.

## Hard exclusions

- No text, letters, numbers, labels, row names, or watermarks.
- No Hebrew or any other in-image language.
- No checkerboard background; use genuine transparent PNG alpha.
- No stars, dust, smoke, motion streaks, impact symbols, glows, shadows, or particles.
- No clothing or equipment baked into the body.
- No invented actions or additional frames.
- No cropped limbs or objects crossing into adjacent cells.
- No changes to canvas layout or frame registration.

## Output

Return one transparent PNG master atlas. Keep every cell aligned so future eyes, hair, pants, shirts, shoes, gloves, armor, and weapon overlays can use the exact same canvas without resizing or repositioning.

