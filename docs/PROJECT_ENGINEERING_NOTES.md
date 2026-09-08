# PURGATORY — Project Engineering Notes

This document preserves decision-relevant engineering context that accumulated during development: manual observations, Cursor implementation reports, architectural conclusions, known rough edges, and tool/design constraints that are useful to remember but too detailed for the active roadmap.

It is **not** the phase authority. Current execution order lives in [`ROADMAP.md`](ROADMAP.md), exact gameplay phase lives in root [`PHASE`](../PHASE), and frozen architecture belongs in [`DECISIONS.md`](DECISIONS.md) / architecture docs.

## Collaboration / implementation workflow

- Repository changes are normally implemented through Cursor after a narrowly scoped engineering prompt.
- ChatGPT acts as engineering partner / reviewer / architect: inspect current repo state, challenge assumptions, define the smallest correct slice, review Cursor reports, and preserve important conclusions.
- Direct GitHub edits from ChatGPT are exceptional and require explicit user approval.
- Fresh Cursor chats + narrow scope + a cheap capable model proved dramatically cheaper than broad implementation chats. A Phase 9E-style focused run was about $0.20 On-Demand while still producing the required proof.
- Prefer subsystem slices and wiring proofs over “build a system” prompts when owners/contracts already exist.

## Development Hub / validation lessons

- Developer Hub is the normal development entry point; `DEV.BAT` remains a fallback shell.
- Runtime/load actions should wait for an explicit server Ready state before launching dependent work.
- If the owned server dies or meaningfully degrades during a run, that run should fail rather than continue producing misleading evidence.
- Interrupted validation/load runs should finalize as **ABORTED**, while owned-process cleanup remains bounded and deterministic.
- Load-mode admission up to 256 is a bounded test-mode capability, not a production player-count promise.
- Performance measurements are baselines / characterization evidence unless a specific gate explicitly defines a PASS criterion.

## Debug overlay / diagnostics

- The debug overlay should be gameplay-transparent by default: opening it must not freeze simulation or disable normal movement/jump.
- Keyboard suppression should occur only while an editable egui control actually owns keyboard focus.
- D4 moved privileged diagnostics behind compile-time feature `dev-diagnostics`; shipping-style `--no-default-features` excludes those diagnostics by compilation rather than hiding them at runtime.
- External diagnostics / observer tooling remains a later direction rather than something silently mixed into the game client.

## Player movement speed debug tooling

- A debug Move speed control exists for local prediction/offline-style client simulation paths.
- When connected to an authoritative server, changing only the local speed does **not** change authoritative movement; server motion remains at its authoritative value and reconciliation will correct large divergence.
- The intended future/debug use is an **authoritative** speed-change tool so effects such as +20% movement speed can be tested through the same ownership path eventual gameplay modifiers will use.
- Do not treat a client-only movement-speed slider as proof of authoritative movement modifiers.

## Rendering / resolution investigation

- The rendering investigation found the CPU-side final screen-space vertex path remained continuous; no obvious vertex/camera quantization was found in that path.
- Manual visual comparison showed **4× MSAA** (Multisample Anti-Aliasing — multiple coverage samples per pixel for smoother polygon edges) was significantly smoother than MSAA Off.
- Render resolution / Render Scale is treated as a **fidelity** control, not a gameplay field-of-view control. The gameplay camera remains a locked world-view relationship; higher pixel resolution/render scale increases image quality instead of revealing more world.
- The current development direction uses **200% Render Scale + 4× MSAA** as the high-quality default and **100% + 4× MSAA** as a performance fallback.
- Earlier “150%” thinking was dropped in favor of the 200% high-quality target.
- Remaining shimmer/visual artifacts should not automatically be blamed on camera/vertex snapping; the known investigation did not prove that mechanism.
- Rendering Foundation should not be reopened merely because Animation Lab has a visual rough edge unless evidence demonstrates a shared runtime problem.

## Animation runtime / Animation Lab

### Proven architecture

- Animation is presentation-only. Gameplay authority does not depend on client animation sampling.
- Runtime path is shared with the authoring tool: authored `.anim` → parser → `AnimationClip` → sample → `LocalPose` / projection → skeleton evaluation.
- A7.0 established standalone eframe Animation Lab authoring with open/edit/save/reload, key add/edit/delete/move, direct bone/channel/time editing, and exact Undo/Redo history semantics.
- Redo history is truncated after branching edits, as expected for a command history.
- Jump/Fall authored files remained parse-compatible through the same runtime path.
- A7.1 added multi-select, batch operations, copy/paste, pose copy/paste, snapping, facing preview, transition preview, and authored `depth_angle` foreshortening.

### Authoring constraints / known rough edges

- Root is intentionally non-keyable in the current format; translation authoring is restricted to selected central bones.
- Key-time storage uses the fixed document grid; editor snapping does not change that storage contract.
- Facing-left and A→B transition are previews, not asset rewrites or a gameplay/runtime state machine.
- When an animation channel has no authored key, runtime fallback is the skeleton bind/default pose. This creates an important authoring UX issue: beginning a new clip from a manually arranged pose does not automatically create movement from bind pose unless start keys exist.
- A useful future tool action is **Set Current Pose as Clip Start**: capture the currently arranged pose as time-zero authored keys so edited bones can visibly transition from their intended clip start.
- Animation Lab still has UI/authoring rough edges and deserves a later dedicated polish pass. These do not by themselves reopen the now-GREEN Rendering Foundation.

## Character Presentation / Phase 8 closeout

- Phase 8 closed with authorization / stale-state regression coverage, including stale-world interaction/portal attempts, old replication epoch, stale entity generation, reconnect sequencing, and duplicate-sequence accounting.
- No unresolved mutating `OPEN_RISK` remained at Phase 8 closeout.
- Character Presentation uses one canonical conceptual path: activity → view → clip → pose → draw plan → visual selection.
- Local predicted and remote interpolated characters share the Character Presentation path rather than maintaining separate local/remote rendering semantics.
- Semantic draw order is owned centrally; attachments inherit semantic layers rather than arbitrary unrestricted numeric Z.
- `hidden_base` is deliberately narrow and should not turn into a generic visibility/state override.
- `ClimbBack` uses authored `climb_back.anim`; invalid parse falls back to bind/no-animation rather than silently pretending to be Idle.

## Character ART Integration — current design direction

These points are planning constraints, not yet completed runtime work unless a later phase/report says otherwise.

- Character ART integration should reuse the existing skeleton, Character Presentation, equipment and animation owners instead of introducing a parallel character renderer.
- A small UI-free / renderer-free `purgatory-character`-style data owner was identified as the preferred place for durable character-sheet data/semantics if that separation is implemented.
- Character-sheet data should explicitly distinguish **pixel crop/source space** from **character canvas/composition space**.
- A source image such as `CHAR.png` is a **catalog sheet**. Its position on the catalog image must never implicitly become the body part's position on the composed character.
- Runtime visual attachments should point to a texture/atlas plus explicit `source_rect`; the authored composition/pivot/slot decides where the crop appears on the character.
- Base body and equipment Paper Doll visuals should converge on the same visual registry / multi-atlas presentation path rather than becoming two render stacks.

## Character Lab / cutting tool — design notes

These are authoring-tool constraints gathered while planning C1–C8.

- `CHAR.png` should be treated as a source catalog sheet, not as a registered frame-based animation atlas.
- The sheet may contain unequal numbers of weapons/variants and should not be forced into a fake uniform grid model.
- Limbs / boots may require explicit front/back crops; the tool should author what exists rather than infer symmetry from catalog placement.
- Character Lab should visually author source rectangles, part identity, slot/composition metadata, pivots/placement and relevant pairing without requiring manual JSON editing.
- The composer exists to expose crop, pivot, overlap and separation errors immediately.
- Catalog coordinates and character coordinates are separate systems.
- The Hub integration was planned around the existing fixed-size development UI constraints (including the 1280×800-style Hub surface seen during planning).
- For the image workspace, Character Lab should opt out of the normal page-wide vertical `ScrollArea`; pan/zoom belongs to the image canvas itself.
- The desired visual style intentionally allows character body parts to appear **disconnected / floating with air gaps** between pieces. The authoring/runtime system must not assume neighboring limb sprites must geometrically touch.
- C-track must not silently absorb rigging, animation authoring, sockets, atlas export or arbitrary Z-order editing.

## Character visual style implication

- The current concept art direction is not a conventional fully connected sprite body. Individual anatomical parts can be separated by visible negative space.
- This makes the skeleton/pivot system more important: visual continuity comes from coherent motion and authored spacing, not from pixels physically meeting at every joint.
- Paper Doll equipment must therefore tolerate and intentionally layer over a character assembled from spatially separated pieces.
- Future hit effects such as per-bone/per-part shake can fit naturally into the existing skeleton/presentation model if implemented as a presentation-layer procedural offset rather than authoritative gameplay state.

## Combat / Phase 9–10 conclusions

- Ability lifecycle reuses the existing Action Runtime rather than introducing an ability-specific scheduler.
- Client requests ability intent; it never chooses authoritative damage.
- Damage flows through `AbilityEffect` / Health ownership; Alive/Dead remains derived from authoritative Health.
- Phase 9E manual proof showed the combat creature pursuing the player, attacks producing the player's Hurt animation, repeated hits producing the current Dead presentation, and the creature stopping pursuit once the player was dead.
- The awkward/frozen-looking Dead visual is a **presentation placeholder**, not a Phase 9E combat failure.
- Phase 10E closed the normal-session PvE loop with NPC Hurt / Dead / Respawn presentation through existing Health/lifecycle events plus end-to-end integration proof.
- Phase 10 is closed; known visual rough edges belong to polish unless new evidence shows a shared runtime failure.

## Phase 6 / scaling conclusions worth preserving

- Core authority path: client intent → validation/owner → GameplayOwner / World tick → relevance / replication → SnapshotBuilder / frame construction → QUIC.
- AOI (Area of Interest) shifted replication from world-wide assumptions toward spatial Enter/Stay/Leave relevance.
- Coalesced state and reliable events are different delivery problems and should not be merged into one queue policy.
- The ~128-client degradation observed earlier was not accepted as proof of a simulation bottleneck; later instrumentation did not reproduce it as a server-domain hang/blow-up on the tested machine.
- Relationship/density/budget policy reduced recorded hotspot stranger updates by about 3× versus baseline.
- Phase 6G closed the **replication architecture**; future production values are tuning unless new evidence proves an architectural defect.

## Known polish / revisit list

- Animation Lab authoring/UI polish pass.
- Better current Dead presentation.
- Character ART / Paper Doll visual integration through ART-R.
- Character Lab / cutting-composition authoring tool.
- Authoritative debug movement-speed modifier path.
- Rendering visual-quality/shimmer follow-up should be evidence-driven; do not assume vertex snapping without proof.
- Production replication-policy tuning remains future work; do not reopen 6G architecture casually.

## Post-Phase-10 gameplay-readiness pass

- Phase 10 remains closed. Phase 11 Item Loop is complete and closed after a
  normal client/server runtime proof. The local Health HUD reads replicated
  Health directly; it does not maintain a second client value.
- Player death remains authoritative Dead until an explicit Respawn intent is
  validated by the server. Respawn reuses `World::respawn_player_entity` and
  rebases the input epoch.
- The live combat creature uses 2 damage against 20 Health, giving roughly
  ten successful hits before death.
- NPC activity currently owns free 2D movement rather than FOOTNOTE platform
  grounding. Correct base-platform placement therefore remains an architectural
  gap, not a visual-offset tuning problem.

## Maintenance rule

Add to this document when a development discussion produces one of the following:

- a proven behavior that is easy to forget,
- a manual observation not obvious from code,
- a reason an architecture choice was made,
- a rejected hypothesis that would otherwise be reinvestigated later,
- an important tool/authoring constraint,
- or a known rough edge deliberately deferred to polish.

Do **not** use this file to override current `PHASE`, active Roadmap ordering, ADRs, protocol contracts, or test reports.
