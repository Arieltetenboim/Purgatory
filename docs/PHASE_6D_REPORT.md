# PHASE 6D REPORT — Runtime Query, AOI, and Replication Relevance

## Status

**GREEN (automated).** Manual runtime verification is **not** user-confirmed.

**TRANSITION PRESENTATION GATE READY FOR USER CHECK**

**CAMERA DEAD-ZONE SMOOTH FOLLOW READY FOR USER CHECK**

**LOCAL PLAYER CAMERA JITTER READY FOR USER CHECK**

**TRANSITION BLACKOUT COMMIT FIX READY FOR USER CHECK**

**TRANSITION INPUT BARRIER READY FOR USER CHECK**

**PHASE 6D PERFORMANCE EVIDENCE READY FOR REVIEW** — [`docs/PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md)

Do **not** start Phase 6E.

## Quality gate

`./scripts/check.ps1` — **passed** on 2026-08-30. Recorded in [`docs/TEST_GATES.md`](TEST_GATES.md).

Commands actually run (fail-fast, in order):

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `cargo run -p purgatory-content-validator -q`

Result: `PURGATORY quality gate OK`. Content validator: maps=2 entities=5 defs=7.

## Implemented scope

- **6D.1** World-owned uniform grid per live `WorldAddress`. Relocate-on-write. Cell size `SPATIAL_CELL_SIZE_WU = 4.0` wu (tunable).
- **6D.2** `World::spatial_candidates` = leave-rect + class. No hysteresis, no `ConnectionId`. AOI policy rects `[16, 9]` + leave margin `2` wu are server interest policy, not client 16:9.
- **6D.3** Per-observer `ObserverReplicationState` (WantEnter / Known / WantLeave), coalescing mailbox, epoch-tagged writer queue cap 4.
- **6D.4** Protocol **v8** `ReplicationFrame` (tag 16: Enter / Update / Leave, `observer_baseline_epoch`, optional health). Protocol **v9** DEV `DevSetChannel` (tag 17). Client `apply_frame` drains every frame. One persistent QUIC uni; second uni is protocol-illegal.
- **6D.5** Size-aware budget (`REPLICATION_FRAME_BUDGET_BYTES = 4096` ≤ `MAX_GAMEPLAY_SNAPSHOT_BYTES = 8192`). Progressive Enter. Updates cannot overtake an uncommitted Enter.
- **6D.6** LoadMetrics schema **2** (enters/leaves/updates/churn, bytes, oldest pending ticks, queue depth). Placement env `PURGATORY_LOAD_PLACEMENT=cluster|spread|maps`. Debug overlay: replica epoch, known count, optional AOI rect gizmos.

Guardrails:

1. Epoch purge after queue-commit: queued old-epoch frames are dropped; observer committed revs / known set reset; new epoch sends a current baseline; purged queue-commit is not treated as delivered.
2. Persistent uni `write_all` failure tears down the session. No partial-frame retry. Never skip the failed frame onto a sibling stream.

## Important files / modules

- `crates/simulation/src/spatial.rs`, `aoi.rs`, `domain.rs`, `world.rs`
- `crates/protocol/src/frame.rs`, `version.rs` (`PROTOCOL_VERSION = 9`, `DEV_CHANNEL_MAX = 1`)
- `apps/server/src/network/replication.rs`, `gameplay.rs`, `handshake.rs`
- `apps/client/src/replica.rs`, `network/runtime.rs`, `app.rs`, `camera_follow.rs`, `local_presentation.rs`, `debug/overlay.rs`
- `crates/common/src/load_metrics.rs` (schema 2)
- ADR-0038, ADR-0039, ADR-0040 in [`docs/DECISIONS.md`](DECISIONS.md)

## Tests added or changed

- Simulation: grid relocate, domain revs, WorldAddress channel/instance/map isolation, Channel spatial relocate, `rebind_map_address` (`phase6d_tests.rs`)
- Server mailbox: `epoch_purge_after_queue_commit_resets_committed_revs`, updates-not-before-enter, coalesce, hysteresis band, progressive Enter (8/frame budget)
- Server channel: two-player isolate/rejoin, invalid Channel no-op, Portal preserves ChannelId
- Protocol: v9 Hello/Welcome/`DevSetChannel` goldens; v1–v8 Hello/Welcome and v1–v7 snapshot goldens frozen; tag 16 roundtrip
- Client: channel/instance change is `RetargetAddress` (no fade); two frames before poll both apply; older epoch ignore; newer epoch reset; unknown Update ignored
- Client presentation: `damped_camera_exposes_raw_prediction_corrections`; `presentation_smoothing_hides_small_corrections_in_screen_space`; large-correction snap; camera/render share `FrameLocalPose`; `steady_right_motion_enters_follow_once`
- Live QUIC: accumulate Enter/Update/Leave; `open_uni` once; write-failure tear-down contract
- Bot: decode `ReplicationFrame`

## Manual / runtime verification still required

1. E interact still opens switch/chest; overlay nearest generic vs portal unchanged.
2. Portal A↔B with fade: no old-map ghosts after travel (epoch reset).
3. Remotes still appear after a brief writer stall (queue drain, not watch drop of Enter).
4. Second client still sees both players inside AOI; far entities leave then re-enter without stuck Known.
5. Two-client Channel gate (same coordinates): both ch=0 see each other; B→ch=1 isolates; A→ch=1 rejoins; B→ch=0 isolates again; Portal on ch=1 preserves ChannelId. Overlay `[0] [1]`. No map reload/fade/pose jump on Channel-only change.

## Deviations from the original plan

None material. `WorldSnapshot` types remain for frozen goldens, interpolation/prediction views, and existing unit tests. Live uni traffic is v8 frames only. Channel control is DEV overlay only (`DevSetChannel`), not a production WindowManager. Persistence of ChannelId is explicitly deferred to 6E.

Live AOI smoke tests place the observer at `x = -8` so the authored Map A portal (`x ≈ 6`) is inside the 16×9 enter rect. At `FOOTNOTE_SPAWN_X` (`-19.4`) that portal is correctly outside the leave rect; tests were aligned to policy AOI rather than expanding AOI or treating missing Transform as globally visible.

## DEV visibility follow-up

Semantic DEV overlay for AOI inspection (still under manual verification; not Phase 6E):

- Known replica entities labeled `LOCAL PLAYER`, `REMOTE PLAYER`, `INTERACTABLE`, `PORTAL` (compact world-space chips). Namespace IDs stay in the inspector. `ContentId` is not on the replica.
- Overlay **Observer AOI**: observer id, WorldAddress, Enter/Leave policy bounds, candidate/Known/WantEnter/WantLeave mailbox counts, replication epoch, plus Known counts per replica kind (Players / Interactables / Portals) and World platform count (`repl=None`).
- World **Entities** inspector is a categorized tree (Players / Interactables / Portals expanded; Platforms / Other collapsed). Each row has labeled `World:` and `Replication:` lines. Local World slots and replica Known-set IDs are not merged by matching `index:generation`. The local player is one semantic inspector/world-space entry with both identity namespaces labeled.
- World-space labels use light text on a dark translucent chip; category color is an accent border, not body text. Gameplay quad colors are unchanged. Labels are hidden while maps are unaligned and reappear only after `DestinationReady` (same presentation gate as FadeIn / local pose).
- Optional v8 8-byte `ObserverAoiDebug` trailer (frozen goldens omit it). Policy-rect bands are presentation only.
- Distinct overlay outlines: local square hat, remote triangle hat, interactable/portal outlines. WantEnter entities are not drawn.

## DEV entity inspector follow-up

World **Entities** was a flat RuntimeEntityId dump dominated by platforms. It is now a categorized inspector (Players / Interactables / Portals expanded; Platforms / Other collapsed) with labeled World vs Replication lines. Local player is one semantic entry (`server RuntimeEntityId` + `client World EntityId`); those namespaces are not peer rows. Still under manual verification; not Phase 6E.

## Portal local-pose transition fix

Manual 6D testing: during A→B the local player appeared at the wrong pose (new-epoch destination coordinates on the old map, then a first-platform spawn on the new map) before snapping to the linked portal.

Root cause: the first incorrect pose entered **client presentation** on new-epoch apply. `prediction.clear()` plus `local_presentation_pose` falling through to the replica used the destination Enter against still-visible Map A geometry. `rebuild_local_map` then spawned at the first platform center and `notify_map_ready` allowed FadeIn before prediction/camera were seeded from that Enter.

Ordering change:

1. Keep old-map prediction/world pose until maps align (do not present new-epoch replica pose on the old map).
2. Server establishes dest Transform, re-grounds, bumps epoch, then publishes the new-epoch baseline **before** the next sim tick. Observer self Enter is encoded first.
3. Client swap waits for self Enter, seeds World+prediction from that pose, follows with the camera, then `DestinationReady { epoch, local_pose }`. FadeIn cannot start without it.

## DEV world-space label jump during portal transition

Manual 6D testing: world-space entity labels jumped sideways around the map/epoch/camera swap.

Root cause: the first mismatched projection was **new-epoch replica poses (destination coordinates) through the still-old camera**. Gameplay quads already gated replica drawing on `maps_aligned`; `replica_label_world` did not. Local labels used old-map prediction while remotes/interactables/portals used dest replica poses — two different transition gates.

Ordering / gate change: `world_space_labels_eligible = maps_aligned && (fade idle || DestinationReady)`. While unaligned, emit no projectable label poses. Restore after destination geometry, self baseline, prediction, and camera share that world. No interpolation across maps.

## Portal re-entry / false-fade fix

Manual 6D testing: after A→B, held Up correctly did not bounce, but a **new** Up press while still standing on the destination portal was rejected until the player walked out of the zone. The client also started FadeOut on `PortalActivate` send, so locked/rejected Up produced a full fade with no map change.

- Reentry lock clears on **Up release** (`InputCommand.portal_held` falling edge). Zone exit remains an extra clear path.
- FadeOut starts only when the replica observer **MapId** changes (accepted map transition). Same-Map Channel/Instance uses a separate membership fade (ADR-0041), not a map rebuild.

## Channel transition gate

Channel change on the same Map is a real `WorldAddress` boundary (`MapId + ChannelId + InstanceId`).

- DEV overlay: prominent `Map / Channel / Instance` and `Channel: [0] [1]`. Sends `DevSetChannel`; the client does not mutate WorldAddress.
- Authority: server `set_address` (same Transform), rematch `grounded_on`, bump observer epoch, purge older queued frames, publish Enter baseline. Player stays Active; same `RuntimeEntityId`.
- Spatial: entity leaves the old-address grid and enters the new-address grid.
- Presentation: membership fade (200/50/250 ms) then `RetargetAddress` / `World::rebind_map_address` while black. No geometry rebuild, no teleport, camera/pose stay. FadeIn waits for `MembershipReady`.
- Server also sends `Interact Closed / AddressChanged` for a live world-bound session (presentation cannot be ready while UI stays Open).
- Portal policy: preserve current ChannelId and InstanceId (content `transition` has no channel field).
- Persistence: Channel transition correctness is 6D runtime semantics. Whether ChannelId is restored across login/restart is a Phase 6E policy. Runtime allocation/load-balancing remains deferred.
- Diagnostics: `6D_CHANNEL` stdout (transition / reject / retarget). No new production metrics.
- `AddressChanged` closes world-bound `InteractionSession` only. WorldAddress is not a social identity (whisper / friends / party / guild are not implemented).

Deferred: production Channel selector, automatic allocation, capacity policy, instance-management UX.

## Unresolved issues / risks

- Health on the wire is a delta-domain proof, not combat.
- AOI constants and cell size are development tunables; Scenario A/B/C numbers are characterization, not capacity claims. See [`docs/PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md).
- In-flight write of an old epoch may still complete; the client ignore-older-epoch rule covers that. A dead replication stream is a session failure.

## 6D.6 load characterization

Release localhost matrix 2026-08-29 (`mixed` / seed 4242 / 45 s / N=10,25,50). Full tables: [`docs/PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md).

- No encode/write failures, no unexpected disconnects, writer queue peak 1, oldest pending 0.
- Scenario A remains the expensive overlap case (N=50: 1.28 MiB/s out, 26 KiB/s/client).
- Scenario B is only weakly cheaper on this 48 wu map (N=50: 0.97 MiB/s, 20 KiB/s/client). Local known set still tracks N.
- Scenario C address split cuts known set roughly in half (N=50: 0.73 MiB/s, 15 KiB/s/client).
- v8 vs reconstructed v7 full snapshots (Scenario B): outbound ≈ **0.5×** at each N — delta encoding, not AOI bounding.
- Claim “global N up, local set bounded ⇒ per-client bandwidth bounded” is **not supported on this development stage**.

## DEV interaction status strip

The overlay header is INTERACTION / TARGET / PORTAL plus WORLD Channel control (DEV `[0] [1]`). Request/result history lives in Network → Interaction. World-space chips are compact semantic names. Transition banners (`MAP TRANSITION · …` / `CHANNEL TRANSITION · Waiting MembershipReady`) are DEV-only. Still under manual verification; not Phase 6E.

## Transition presentation gate

**Transitions are readiness-gated, not timer-revealed.** Fade duration is presentation timing; FadeIn readiness is state-driven (ADR-0041).

- Map: FadeOut 300 ms, min hold 75 ms, FadeIn 400 ms. `DestinationReady` after dest address, epoch, geometry, self Enter, seeded pose, prediction, camera, old-map replicas gone.
- Channel/Instance: FadeOut 200 ms, min hold 50 ms, FadeIn 250 ms. `MembershipReady` after dest membership, epoch, old Known gone, dest Enter, stable pose, matching presentation address, world-bound session closed. Not a map load.
- If readiness is late, remain black. DEV stall at 5 s logs missing flags; does not fake success.
- Simulation and replication continue while obscured.
- **Gameplay input barrier (ADR-0042):** held movement is neutralized on transition accept; the player stays at the dest Portal (or Channel pose) until FadeIn. Client unlock is FadeIn; server lock covers FadeOut + min Hold in ticks.

## Camera Dead Zone + smooth follow

Client presentation only. Not AOI. Not simulation.

- Dead Zone containment around the current camera center (`DEAD_ZONE_HALF_X = 2.0` wu, `DEAD_ZONE_HALF_Y = 4.0` wu). Crossing an edge moves the desired center only enough to keep the player at that boundary.
- Exponential damping with render `dt`: `1 - exp(-dt / smooth_time)`. `SMOOTH_TIME_X = 0.20` s, `SMOOTH_TIME_Y = 0.40` s. Axes independent. No per-frame lerp factor.
- Map `DestinationReady` path snaps the camera to the dest pose. Channel/Instance seeds from the current pose (no jump, no cross-map travel).
- Overlay Camera tab + optional Dead Zone gizmo (hidden when debug is closed). DEV jitter lines: predicted / replica / presented / camera / screen X, correction this frame, 90-frame screen-X range, follow-X flips.

## Local player camera jitter (smooth-follow exposure)

Manual Dead Zone test: once horizontal follow started, the local player flickered in screen X. The same mover was smooth as a remote on the second client.

**Cause (forensic, runtime NDJSON):** Camera and local draw already share one pose per frame. Reconcile offset was `0` while walking. Predicted/presented X held between 30 Hz ticks (`Δpresented = 0` on idle frames, `≈ 0.20` wu on tick frames = `6 wu/s × 1/30 s`). The camera damps every render frame. Screen X therefore sawtooths: jump on tick, then ease back as the camera catches a frozen sprite. `LocalPresentation` only hid restore+replay pops; it does not fill locomotion between ticks.

**Fix:** Remainder extrapolation `presented = tick_pose + velocity × clock.remainder()`, then the existing correction offset. The world-space `LOCAL PLAYER` overlay chip uses that same presented pose (not tick/replica X). Remainder `0` is the tick pose (no added input delay). Does not feed simulation, commands, reconciliation, or AOI. Remotes still use the delayed interpolation buffer. Camera tunables unchanged. Hitch/map/teleport still snap. DEV **Presentation RawPrediction** still draws the raw tick pose.

## Transition blackout commit

Cause: dest-epoch `ReplicationFrame` was applied (and interp cleared) at the start of FadeOut, so `replica_matches_local_map` became false while the source scene was still visible. Remotes/interactables/portals dropped out mid-fade.

Fix: capture a frozen source presentation **before** dest apply; draw it while `holds_source_presentation()` (FadeOut). Commit (drop freeze, swap geometry/membership, seed camera) only when `is_fully_black()` (Hold). Replica/network/simulation are not delayed. Authoritative `InteractionSession` close is not delayed. STALL still stays black until a real ready (and does not FadeIn if stalled).

## Transition gameplay input barrier

Previous stale-input behavior: `SessionInput` kept last held axis and continued it when the queue was empty. Portal/Channel `bump_epoch` already cleared that for the *old* epoch, but the client kept sending held Right/Left on the *new* epoch during FadeOut/blackout. Local prediction also advanced. First visible dest frame could already be off the Portal.

Server: on accept, neutralize queue/held axis, zero velocity, `InputGateReason::{Map,Membership}Transition` for FadeOut+Hold ticks. Gated ticks ack commands as idle. `PortalActivate` / `InteractOpen` / `DevSetChannel` rejected. Stale old-epoch commands remain `OldEpoch`.

Client: lock from accepted dest address (or FadeOut/Hold) until FadeIn. Sample idle movement; discard jump/interact/portal edges; keep held Left/Right latched. Unlock is FadeIn, not a client timer.

DEV overlay: `INPUT: ACTIVE` / `INPUT: LOCKED · MAP TRANSITION` / `INPUT: LOCKED · CHANNEL TRANSITION`, plus `authoritative movement neutral: yes` while locked.

## Boundary

Work stopped at Phase 6D. **Do not begin Phase 6E.**
