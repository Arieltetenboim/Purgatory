# NPC Dialogue Presentation Runtime Contract

Status: N9 authoring contract plus implemented N10 client projection. The
current Text v0, bubble, facing and animation ownership is documented in
[`NPC_DIALOGUE_RUNTIME.md`](NPC_DIALOGUE_RUNTIME.md).

## Purpose

NPC dialogue lines may carry optional presentation cues without moving presentation ownership into gameplay state or the server simulation.

Current authored line shape already permits:

```json
{
  "text": "Look over there.",
  "voice": null,
  "animation": "point"
}
```

Both `voice` and `animation` are optional logical references. `null` means no authored cue and is a normal/default state.

## N9a — animation authoring

NPC Lab PRESENTATION owns convenient authoring of the existing optional `line.animation` field.

Rules:

- `animation` is optional; `null` / No animation is valid and normal;
- the value is a logical authored animation id, not a filesystem path;
- the picker is populated from the repository catalog by scanning `content/shared/animations/**/*.anim`;
- the current animation id is the `.anim` file stem, matching the existing A6 animation authored-id convention;
- NPC Lab TEST may display the selected cue as preview metadata, but N9 does not dispatch animation into the game runtime;
- dialogue selection, conditions, actions, progression and persistence do not depend on animation playback.

The catalog API is intentionally generic. Additional authoring references such as Items should reuse the same repository-backed catalog primitive rather than introducing one endpoint or hand-maintained list per domain. JSON-backed domains should resolve their canonical authored `id` from the content file rather than deriving identity from filenames.

## N10 — implemented runtime projection

N10 projects authoritative dialogue state into client presentation without
making presentation assets authoritative gameplay data.

Implemented path:

```text
authoritative dialogue progression
  -> semantic active NPC / beat / line identity
  -> client resolves authored line
  -> optional line.animation
  -> existing Animation Runtime / presentation owner
```

The network contract should prefer compact semantic dialogue identity rather than filenames, animation frames, bone transforms, or presentation timers.

Required fallback behavior:

- no animation cue -> keep normal NPC presentation;
- missing/unavailable animation asset -> report/fallback locally and continue dialogue;
- failed animation playback -> continue dialogue;
- gameplay truth never waits for animation completion unless a future explicit gameplay contract is designed for that purpose.

N10 reuses the existing animation parsing/playback/presentation path. It does
not create a parallel NPC-only skeletal animation runtime merely for dialogue
cues.

## Voice follow-up

Voice should follow the same separation when added:

```text
semantic active line -> optional line.voice -> client audio presentation
```

Voice playback failure must likewise never block dialogue/gameplay progression.

## Non-goals for N9a

- game runtime dialogue integration;
- voice authoring/playback;
- facial expressions;
- camera cues;
- sound-effect cues;
- graph editing;
- changing NPC gameplay behavior.
