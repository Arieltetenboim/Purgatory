# PURGATORY — First Playable Slice

## Status

**Working design brief.**

This document records the current design direction for PURGATORY's first playable slice and opening area, provisionally referred to as **Welcome** / **"ברוך הבא"**.

Unlike [`GAME_VISION.md`](GAME_VISION.md), this document is expected to evolve as the area, story, quests, enemies, items, abilities, and runtime requirements are discovered.

Do not treat unresolved details here as frozen lore or architecture.

---

## Purpose of the slice

The first playable slice should be a small but coherent piece of the actual game, not a detached tutorial.

It should simultaneously:

- introduce the player to the tone and lived reality of PURGATORY;
- teach core interactions through normal activity in the world;
- provide a controlled first experience of exploration, combat, items, NPC interaction, quests, and progression where appropriate;
- function naturally as a multiplayer space for new characters;
- begin the main narrative without presenting a lore lecture;
- expose concrete runtime requirements for later development;
- end by releasing the player into the ordinary persistent game world.

The slice should be designed as a real place and sequence first. Systems should then be derived from what that experience demonstrably requires.

---

## Narrative relationship to the game vision

The opening should serve as the player's first lived encounter with PURGATORY's central question:

> **How do we live meaningful lives in a world that promises to preserve nothing?**

Welcome represents an attempt to create normality and durable human life in a world where permanence cannot be guaranteed.

The place should not be a joke, fraud, or obviously doomed settlement. It should have **actually worked for some meaningful period of time**.

People live there. Work is done. Things are repaired. Food is prepared. Trade happens. Relationships exist. People make plans. Some residents may genuinely believe they have finally created something stable enough to last.

Its later loss should not prove that building it was pointless.

A desired retrospective feeling is:

> **It was fragile. It did not last. It was still worth having existed.**

---

## What Welcome is

The strongest current direction is that Welcome began as a **small transit point** and gradually became more than that.

People arrived intending to stop temporarily, travel onward, find work, wait for safer passage, obtain supplies, or begin again somewhere new. Some stayed. Services appeared. Structures became homes. A temporary stop accumulated enough continuity to begin thinking of itself as a settlement.

Over time, Welcome developed a reputation as a place where a person could find a little stability or make a new start.

Its identity may be shaped by a founder, organizer, or community ethos that insists on ordinary life: maintain the walls, repair what breaks, greet newcomers, keep working, plan for next season. That attitude should remain morally ambiguous in a productive way. It may contain denial, but it may also be exactly what allowed the community to function for as long as it did.

The provisional name **Welcome** may be literal, informal, ideological, ironic, or some combination. Its final in-world naming should be decided later.

---

## The player character is not new to the world

The player character already belongs to PURGATORY's world.

The user is new; the character is not.

Therefore the opening should avoid the common device in which NPCs explain ordinary world facts to the protagonist solely for the user's benefit.

Characters may casually reference places, events, groups, dangers, customs, or terminology that the player has not yet learned. Meaning should accumulate through context, environmental detail, item text, behavior, and later encounters.

The opening is not the character's arrival in existence or first exposure to the world's basic reality.

---

## Why the character came to Welcome

This is **not yet frozen**.

The preferred broad direction is intentionally compatible with many player identities:

- Welcome is a known transit point and place to begin again.
- The character has arrived because they are moving through life, seeking work, opportunity, passage, a new beginning, or simply the next place to go.
- The exact personal reason does not need to be strongly prescribed at character creation.

Possible interpretations that remain compatible with this direction include:

1. **Starting again** — the character came because Welcome accepts people with little ceremony and offers a chance to establish themselves.
2. **Passing through** — the character expected only a temporary stop on the way elsewhere.
3. **Following a contact** — a person, recommendation, letter, or opportunity drew the character there.
4. **Work** — Welcome needs labor, guards, couriers, gatherers, repair work, or other practical help.
5. **No dramatic backstory** — the character stopped because it was a reasonable place to stop and stayed long enough for the opening events to matter.

A useful default may ultimately combine **transit point + opportunity to start again**, while leaving the player's personal motivation open.

---

## Multiplayer structure

Welcome is intended to be a **shared multiplayer area for new characters**, not a one-shot cohort instance that requires several players to begin together.

Players may be present at different stages of their own opening progression at the same time.

This is important because PURGATORY should not assume a constant stream of many simultaneous new players.

The area therefore needs to feel inhabited and socially plausible with very low new-player concurrency.

Design implications:

- keep the useful space relatively compact rather than building a large "newbie city";
- prefer a few naturally intersecting paths and activity centers;
- use NPC routines, environmental activity, sound, and density to make one-player occupancy feel intentional rather than empty;
- two or three players should already make the area feel socially active;
- seeing another new player at a different point in progression is acceptable and potentially desirable.

The opening should not depend on a synchronized group event.

---

## Welcome is one-way for the character

The player character can inhabit Welcome during the opening progression, but after the final transition **cannot return**.

This is a character progression boundary, not necessarily a globally synchronized destruction state.

A character who has left no longer has a valid gameplay route back to Welcome.

Meanwhile, other new characters may still be inside their own opening progression and see the ordinary pre-loss version of the area.

The design therefore does not require all concurrent players to observe the settlement's destruction simultaneously.

The exact runtime representation of this rule should be derived later from existing map, character, persistence, quest/progression, and world-address ownership rather than assumed here.

---

## The loss of Welcome

Welcome ultimately ceases to exist as a place the player can return to.

The current preferred structure is **not** to show the player a synchronized cinematic collapse while they are standing inside it.

Instead, the character is sent out on what appears to be a relatively ordinary task — currently imagined as a delivery, message, object, report, escort, or similar practical errand.

The task moves the character out of Welcome and into the ordinary game world.

The character does not necessarily understand that this departure is permanent.

While the character is away, Welcome is lost.

The player learns this later, after enough distance has formed for the news to carry weight.

This allows the area to remain operational and multiplayer-friendly for other new characters while preserving the irreversible narrative transition for the individual character.

---

## The outbound mission

The final Welcome mission should feel mundane enough that leaving does not announce itself as "the end of the tutorial."

A strong pattern is:

**ordinary request → journey outward → first uncontrolled reality → disruption → destination → delivery / completion → news of Welcome**

The exact payload and destination remain open.

The mission should provide a natural reason for the character to cross from the protected opening space into the normal persistent world.

Possible payloads include:

- a physical item;
- a message or report;
- medicine or supplies;
- a repaired object;
- a document;
- information that becomes more important after Welcome is lost.

The payload may later become narratively significant, but it does not need to begin as an obviously important artifact.

---

## First death / time discontinuity — candidate, not commitment

One promising but **unresolved** idea is to use the outbound journey for the character's first controlled encounter with death or with the world's unusual relationship to continuity and time.

A possible structure:

- the character is overwhelmed or killed;
- normal temporal continuity becomes unclear;
- the character returns, wakes, respawns, or otherwise continues according to whatever death semantics PURGATORY ultimately establishes;
- the game does not provide a precise account of how much time passed;
- the character continues toward the destination;
- the temporal gap helps make the later loss of Welcome plausible without requiring the player to witness it directly.

This should only be adopted if death, continuity, or altered time perception are genuine parts of PURGATORY's broader world logic.

It must **not** become lore merely to solve a staging problem.

---

## Discovery that Welcome is gone

The player should not learn the truth immediately after crossing the boundary.

The outbound journey should create some emotional and geographic distance first.

At or after the delivery destination, another character or piece of evidence reveals that Welcome is no longer there.

The preferred emotional effect is initially smaller and stranger than a grand apocalypse reveal:

- surprise;
- disbelief;
- uncertainty about exactly what occurred;
- recognition that there is no route back;
- realization that the place the player had only just begun to understand is already part of the past.

The cause of the loss is deliberately **not fixed yet**.

Different reports may even disagree.

The opening can plant a detail whose significance is understood only much later, allowing the first area to be reinterpreted retrospectively as part of a wider pattern.

---

## Warning signs before departure

Welcome should not look obviously doomed.

If every wall is collapsing and every NPC predicts disaster, the later loss has no power.

Instead, use individually ordinary signs that become meaningful only in retrospect:

- a repair that keeps failing;
- a route temporarily closed;
- a shipment that has not arrived;
- a person who has not returned;
- worsening conditions beyond the safe edge;
- a missing resource;
- a disagreement about whether conditions are unusual;
- a small structural or environmental anomaly;
- an authority figure insisting the problem is manageable.

No single sign should function as a countdown.

The player should later be able to think, "there were signs," without reasonably having been expected to predict the outcome.

---

## People of Welcome

The residents should embody different responses to fragility without becoming philosophical mouthpieces.

Useful roles may include:

- a founder / organizer who believes deeply in maintaining normal life;
- a practical worker who keeps the place functioning rather than debating its meaning;
- someone who expects Welcome to fail and refuses to become attached;
- someone who arrived temporarily and chose to stay;
- a person preparing to leave;
- a resident whose future plans make the later loss emotionally concrete.

The founder or leader should not be written as a fool merely because Welcome eventually falls.

If the settlement genuinely helped people and survived for meaningful time, their project had real value even if their belief in permanence was mistaken.

---

## High-level player journey

The current structural target is:

1. **Ordinary life / arrival context** — Welcome feels like a functioning place rather than an emergency scene.
2. **Orientation through activity** — movement, interaction, people, space, and basic systems are learned through ordinary tasks.
3. **First responsibility** — the player becomes useful to someone rather than receiving abstract tutorial instructions.
4. **Controlled exposure to danger** — the outer edge introduces combat, risk, items, or other core gameplay.
5. **Small success** — the player earns something: equipment, knowledge, trust, access, or capability.
6. **Subtle instability** — individually plausible signs suggest that Welcome is less secure than its culture claims.
7. **Outbound task** — the player is asked to carry something or complete a practical task outside.
8. **Threshold crossing** — the character enters the ordinary game world without being told that the opening has ended.
9. **Disruption** — possibly a first death, dangerous encounter, or other loss of certainty.
10. **Destination / completion** — the outbound task is completed.
11. **News** — the player learns that Welcome is gone.
12. **No return** — the character's opening state is permanently closed.
13. **Full game** — the character now has motivation, history, possessions, relationships, and unresolved questions inside the normal persistent world.

This sequence is a design scaffold, not a fixed quest script.

---

## Content scale

The slice should remain deliberately small.

The goal is not to make a complete starter continent, faction campaign, or miniature version of every future system.

A useful target is a compact combination of:

- a handful of memorable NPCs;
- a small number of meaningful locations;
- a few enemy behaviors with distinct learning value;
- a limited first item set;
- a short quest chain or interlocking set of practical tasks;
- only enough abilities / progression to communicate the combat language;
- one irreversible transition into the full game.

Each content element should justify itself through at least one of:

- **Game** — improves the actual playable experience;
- **World** — reveals something meaningful about PURGATORY;
- **Probe** — exposes a concrete runtime/system requirement.

---

## Requirement extraction rule

Once a piece of content is proposed, describe the required player-visible behavior before designing a new system.

Examples:

- "The NPC recognizes that the player completed the delivery."
- "The player carries a specific item across maps."
- "An enemy drops an item that can be picked up and retained."
- "A quest observes that a particular enemy was defeated."
- "The character cannot return to Welcome after the transition."
- "Several new characters can occupy Welcome at different progression states without invalidating one another."

Then classify each requirement as:

- **existing capability**;
- **extension of an existing owner / contract**;
- **missing capability**.

Do not jump directly from a content requirement to a new manager, service, crate, or generalized framework.

The first playable slice is intended to **discover the minimum systems the actual game requires**.

---

## Explicitly open questions

The following should remain open until additional design work provides evidence:

- the final in-world name of Welcome;
- its exact geography and visual identity;
- the exact reason the player character came there;
- the identity and history of its founder / leadership;
- the exact cause of its loss;
- whether the loss was unusual or part of a known broader pattern;
- whether first death occurs during the outbound journey;
- what death means in PURGATORY;
- whether time discontinuity is real, perceived, or absent;
- the final outbound mission payload and recipient;
- which NPCs survive or reappear later;
- which clue from Welcome becomes important to the wider plot;
- the exact runtime mechanism enforcing the one-way transition.

These are design opportunities, not missing implementation tasks yet.

---

## Success condition for the opening

At the end of the slice, the player should not feel that they have "completed the tutorial."

They should feel that something happened to **their character**.

They should understand enough of the game to act independently, possess a small personal history inside the world, have lost a place they had begun to recognize, and carry at least one reason to keep moving forward.

The opening should leave the player with the first practical version of PURGATORY's central idea:

> **The fact that Welcome did not last does not mean it was meaningless.**
