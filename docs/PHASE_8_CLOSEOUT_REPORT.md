# Phase 8 closeout

Status: **GREEN**. Root `PHASE` = `8.closeout`. Protocol **v14 unchanged**. **Phase 9 was not started.**

## 1. ClimbBack duration

**Root cause:** `content/shared/animations/dev/climb_back.anim` authors `duration 5.00`. `CLIMB_BACK_CLIP_DURATION = 3.0` duplicated that value in Rust and the test asserted against the constant, so the authored clip failed the gate.

**Fix:** removed the public duration constant. `climb_back_clip()` keeps the parsed `AnimationClip` duration. Invalid-parse fallback uses a private bind/no-animation duration (`1.0`), not the authored length. Tests compare `clip.duration()` to a fresh parse of the `.anim` and sample pelvis `tx` at an authored key that leaves bind.

The `.anim` was not edited.

## 2. Phase 8 audit

Verify-only. No reimplementation.

| Check | Result |
|---|---|
| Local/remote share Character Presentation | **PASS** — adapters at the edge; one `CharacterPresentationSet` |
| Activity → view → clip, including ClimbBack → Back | **PASS** — `view_for_activity` + `clip_for_playback_activity` → `climb_back_clip()` |
| Headwear 1–4 uses existing Crown attachment path | **PASS** — `compose_attachment` (`world ∘ ANCHOR_CROWN ∘ correction`); overlay cell is DEV-only |
| Draw order / visibility / anchors / `hidden_base` | **PASS** — one `PresentationLayer` table; Back hides Front limbs; missing Back omits |
| DEV controls stay DEV-only | **PASS** — Force ClimbBack / Force Back / Headwear 1–4 behind `dev-diagnostics` |
| Render policy 200% default / 100% fallback / 4× MSAA | **PASS** — `RenderScale::DEFAULT = P200`, `PERFORMANCE_FALLBACK = P100`, world MSAA 4× when supported |
| Render Scale does not affect FOV / world visibility | **PASS** — scale rebuilds the offscreen target only; camera FOV unchanged |

**Phase 8 audit: PASS**

## 3. Cleanup

Clear residue only:

- Removed duplicated `CLIMB_BACK_CLIP_DURATION` (and the test that treated `3.0` as source of truth).
- README Graphic sentence was stale vs the documented Headwear Side compile-embed; aligned it.
- Shipping clippy: `FootnoteConfig` import was DEV-only (`apply_debug_move_speed`); gated behind `dev-diagnostics`.
- Lab `committed_png_matches_generator` treated generated example art as source of truth; extracted PNG is artist/Hub grid crop. Replaced with a 256×256 cell check.

Kept: DEV overlay diagnostics, Headwear Side proof on the Crown path, A1 head sample helper, 8F-A..F lock tests.

No opportunistic refactors. Graphic proof PNGs still compile-embed into the client because the world shader always binds group 1; that was an accepted DEV proof, not deleted here.

## 4. Tests

Commands actually run (2026-09-04):

| Command | Result |
|---|---|
| `cargo test -p purgatory-animation` | **58 passed** |
| `cargo test -p purgatory-animation climb_back` | **PASS** |
| `cargo test -p purgatory-client` (DEV / default features) | **587 passed**, 1 ignored |
| `cargo clippy -p purgatory-client --all-targets --all-features -- -D warnings` | **PASS** |
| `cargo test -p purgatory-client --no-default-features` | **523 passed**, 1 ignored |
| `cargo clippy -p purgatory-client --no-default-features --all-targets -- -D warnings` | **PASS** |
| `./scripts/check.ps1` | **PASS** (`PURGATORY quality gate OK`) |

## 5. Remaining non-blocking debt

- No production ART/atlas; debug placeholders + Headwear Side proof sprite.
- No gameplay climb; ClimbBack is DEV-forced or a future server activity.
- No equipment product UI.
- Headwear proof PNGs compile-embed in shipping as well as DEV (cell selector is DEV-only). Optional later: dummy 1×1 bind group in shipping.
- A3–A5 still have Rust duration constants used as invalid-parse fallbacks (out of this ClimbBack fix).
- Residual 6B–6F two-client manuals remain evidence debt, not Phase 8 reopeners.

## 6. Phase 9 readiness

**READY FOR PHASE 9: YES**

Another cleanup pass is **not required** before Phase 9. The Graphic-in-shipping embed is optional follow-up, not a Phase 8 blocker.

Do not start Phase 9 from this report.
