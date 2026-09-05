# Performance budgets

Living budgets. Values may be `TBD` until a phase produces measurements.

The 30 Hz / ~33.33 ms figures below are **tick spacing**, not permission for simulation code to consume 33 ms of CPU.

| Metric | Budget | Current | Notes |
|---|---|---|---|
| Server simulation tick rate | 30 Hz | 30 Hz configured | Initial engineering value, not a permanent promise. Constant: `TICK_RATE_HZ`. |
| Wall interval per simulation tick | ≈ 33.33 ms | 33_333_333 ns | `1_000_000_000 / 30` ns. Spacing between ticks, not a CPU allowance. |
| CPU time per simulation tick | TBD | schema-3 `tick_work_*` + 6G.2 / 7.1–7.5 domain artifacts | Profile p50/p95/p99 before setting. Must stay well below the 33.33 ms wall interval. Domain breakdown is file artifacts, not additional UDP fields. Observation (localhost, idle@256 Release): 6G.2 tick p99 ≈ 16 ms / AOI ≈ 13 ms; after 6G.3 steady-interest skip ≈ 8.0 / 5.8 ms; after 6G.7C hotspot selective @256 tick p99 ≈ 4.8 ms. 7.2 smoke (representative-mixed@8): tick p99 ≈ 0.51 ms, dominant `npc_activity`. **7.5:** mixed@128 util ~8–11%, dominant `replication_policy`; dense@64 util ~10%; unattributed residual ~15–25% (timer noise after Instant span close) — **not** budgets. See [`PHASE_75_REPORT.md`](PHASE_75_REPORT.md). |
| Process CPU utilization | TBD | 6G.2 / 7.1 `process_resources.json` + `capacity_live.json` | `cpu_utilization_pct` is raw (**100 = one logical core busy**). `cpu_normalized_per_logical_pct` is machine-relative (**100 = all logical cores busy**). Observation: ladder cells often 0–80% of one core on a 20-logical machine. |
| Max catch-up elapsed time per outer update | 1 s | 1 s | Surplus elapsed time is discarded. See ADR-0011. |
| Client FPS | TBD | independent of 30 Hz sim | Observed on this machine: render frames >> simulation ticks |
| Horizontal max ground / air speed | 6 world units / s | `FootnoteConfig::max_ground_speed` / `max_air_speed` | Integrated as accel toward target, not snap-to-speed |
| Ground acceleration | 40 world units / s² | `ground_acceleration` | Several ticks to reach max from rest |
| Ground deceleration | 50 world units / s² | `ground_deceleration` | Idle friction on ground only |
| Air acceleration | 20 world units / s² | `air_acceleration` | Air idle does not kill horizontal velocity |
| Gravity | 36 world units / s² | `FootnoteConfig::gravity` | `+Y` up; gravity decreases `velocity.y` |
| Jump velocity | 13 world units / s | `FootnoteConfig::jump_velocity` | Instantaneous; grounded only; preserves `vx` |
| Player FOOTNOTE tick allocations | 0 heap | stack `PlatformScratch` (cap 64) | No `Vec` in ordinary player movement path; cap raised for Phase 4.6 arena |
| Contact epsilon | 0.001 wu | `CONTACT_EPSILON` | Touch ≠ penetrate; ceiling snap separates by epsilon |
| Max Solid recovery translation | 0.5 wu | `MAX_RECOVERY_TRANSLATION` | Exceptional path only |
| Collision history (client debug) | 16 events | ring buffer | Presentation-only; detector OFF by default |
| Client parallax layers | 3 primitive layers | far 0.15 / mid 0.40 / near 0.70 | Presentation-only; batched into existing quad upload |
| Debug time scale | 1.0 / 0.5 / 0.25 | wall elapsed × scale → clock | Does not change `TICK_RATE_HZ` |
| Bytes/sec/player | TBD | 7.4 mixed@64 ~3.5 KiB/s/client; mixed@128 ~4.3; dense@32 ~8.4 (app outbound) | Localhost Release selective/high; see [`PHASE_74_REPORT.md`](PHASE_74_REPORT.md). Not a capacity claim |
| Snapshot entity decode bound | 256 | `MAX_ENTITIES_PER_SNAPSHOT` | Mechanical bound; byte cap still 8192 |
| Replication frame soft budget | 4096 bytes default | `REPLICATION_FRAME_BUDGET_BYTES` / env override | Pre-commit encoded size; wall 8192. 7.4: binds near ≤1024 on mixed@64; default 4096 unbound. |
| Writer queue cap | 4 frames | `WRITER_QUEUE_CAP` | If full, intent stays pending; no encode-and-drop. 7.4 envelope: depth max 1, push fails 0 (localhost). |
| Spatial grid cell size | 4.0 wu | `SPATIAL_CELL_SIZE_WU` | Initial tunable, not an architectural invariant (ADR-0038) |
| AOI enter | view envelope + 2 wu prefetch | `AOI_PREFETCH_MARGIN` | Server-derived from observer pose, FOOTNOTE viewport, Dead Zone, camera clamp. Not client-authored camera. |
| AOI leave margin | +2 wu beyond enter | `AOI_LEAVE_MARGIN` | Hysteresis band; ObserverReplicationState still owns Known/WantLeave |
| Snapshot build cost | local interest | 6D A/B N=50: tick mean ~0.95 ms, p99 ~6 ms; C N=50 mean 0.62 ms | `publish_observer_frame` peak ~4 ms. Localhost mixed 45 s. Not capacity. [`PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md) |
| Remote interp delay | 3 ticks (~100 ms) | `INTERPOLATION_DELAY_TICKS` | Client presentation only; uses `TICK_DURATION` |
| Remote interp history | 16 samples | `INTERPOLATION_HISTORY_CAP` | Bounded; ~0.5 s at 30 Hz |
| Interp teleport snap | 8 wu | `INTERPOLATION_SNAP_DISTANCE` | Presentation tuning, not gameplay |
| Local prediction | 1 player | `LocalPrediction` + client `World` | No full World clone; no remote prediction |
| Prediction re-anchor | 8 wu | `PREDICTION_REANCHOR_DISTANCE` | Hard snap only; no soft reconciliation |
| Prediction catch-up | ≤ `MAX_CATCH_UP_TICKS` | shared `SimulationClock` | Same bound as sim clock; no unlimited loop |
| Connected players | TBD | ≥2 network sessions; load harness up to admission | Remotes interpolated; local predicted; load mode admission ≤256 |
| Tick work overrun | server work > 33.333 ms | recorded; not auto-WARN | Phase 5.7: one overrun ≠ classification WARN; sustained server pressure may WARN |
| Bot scheduler cadence | harness wall clock | `bot_scheduler_*_ms` in metrics.csv | Not server sim cost; not a WARN by itself |
| Load metrics export | localhost UDP | `127.0.0.1:5002` | Off-protocol `LoadMetricsV1` **schema 3** (`PURGSTAT`); missed poll ≠ zeros; rates from counter deltas. Domain timings stay run artifacts, not datagram fields. Do not bump the metrics schema from this document. |
| Soak duration | configurable | Mixed preset default 30 min | Evidence run length, not a permanent quality threshold or the only soak definition |
| Scheduler live slots | 4096 | `SCHEDULER_CAPACITY` | Rejects new work rather than growing unbounded |
| Critical scheduler drain ceiling | 1024 / tick | `CRITICAL_DRAIN_CEILING` | Pathological-overload safeguard; remainder carries forward; not a gameplay budget |
| Deferred scheduler drain | 32 / tick (min 1) | `DEFERRED_DRAIN_BUDGET` | FIFO progress guarantee; `ceil(N/B)` ticks to drain N due jobs |
| Handshake timeout | 5 s | `HANDSHAKE_TIMEOUT` | Hello/Welcome incomplete; not idle timeout |
| Idle timeout | 15 s | `IDLE_TIMEOUT` | Explicit Quinn `max_idle_timeout`; transport liveness, not AFK |
| Control message max | 4096 bytes | `MAX_CONTROL_MESSAGE_BYTES` | Length prefix checked before alloc |
| Datagram payload max | 256 bytes | `MAX_DATAGRAM_BYTES` | Ping/pong only in Phase 5.0 |
| Ping cadence | 1 s | `PING_INTERVAL` | Datagram nonce only; not a gameplay timer |
| Outstanding ping nonces | 4 | drop oldest | Lost Pongs cannot grow a map |
| Concurrent handshake/session tasks | 32 | `NetworkAbuseConfig::max_inflight_connection_tasks` | Admission/concurrency safety, **not** MMO player capacity. Excess `Incoming` refused |
| Max concurrent bidi streams | 1 | abuse policy | Control stream only; extra streams refused by transport |
| Malformed complete-control budget | 32 | per connection | Then disconnect that peer |
| Invalid datagram budget | 16 | per connection | Then disconnect that peer |
| Control messages / window | 8 / 1 s | server `Instant` | 16 rate-drops then disconnect |
| Network diagnostic history | 48 events | `NETWORK_HISTORY_CAP` | Lifecycle/failure ring; no RTT rows |
| Client command queue | 8 | `CMD_CAP` (Connect only) | Disconnect/Shutdown use the watch control plane, never this queue |
| Client lifecycle queue | 16 | `LIFECYCLE_CAP` | Reliable; telemetry saturation cannot hide a disconnect |
| Client telemetry queue | 32 | `TELEMETRY_CAP` | Droppable; drops are counted |
| Peak sessions under soak | 10 | `SessionTable::high_water`, 10×100 extended soak | Localhost convergence check, **not** a player-capacity claim |
| Peak inflight tasks under soak | 10 | `max_inflight` ≤ admission cap | Debug, Windows MSVC, 2026-08-27: 1000 sequential cycles ≈ 31 s, 10×100 multi-client ≈ 6 s, 8-seed × 120-op chaos ≈ 3.5 s. Do not optimize from Debug timing |
| Active entities | TBD | 1,000 / 10,000 lifecycle tests | Debug, Windows MSVC, 2026-08-26: 1,000 tiny platforms spawn ≈ 86 µs, iterate ≈ 27 µs, despawn ≈ 58 µs. Not a player-count claim. Do not optimize from Debug timing. |
| Memory | TBD | TBD | Server and client reported separately |
| CPU | TBD | TBD | Include machine spec with every measurement |
| Equipment full state (all empty) | 1 byte | `EQUIPMENT_FULL_EMPTY_BYTES` | Occupied-mask only; measurement codec, not protocol v11 |
| Equipment one-slot equip delta | 10 bytes | `EQUIPMENT_DELTA_EQUIP_BYTES` | Changed-mask + presence + `ContentId` token |
| Equipment one-slot unequip delta | 2 bytes | `EQUIPMENT_DELTA_UNEQUIP_BYTES` | Changed-mask + presence `0` |

## Recording rules

- Write measurements when a phase affects scale.
- Include machine specification, build mode, scenario, and observed bottleneck.
- Do not claim MMO scale from an unmeasured prototype.
- Database latency must not stall the simulation tick.
- Do not treat the tick interval as a CPU budget.

The previously observed load-only stall around 128 clients is **not** an established server bottleneck and is **not** a budget. 6G.2 did not reproduce a server-domain hang; 6G.7B/C showed replication remaining cheap at 128 and healthy server-side ticks at 256 under the validated workload. Higher-load saturation/timeouts are unattributed (simulation/tick vs server transport vs harness/client). Client `pending_window_stall_ticks` counts skipped predicted ticks when the 128-command pending window is full. Do not raise `PREDICTION_PENDING_CAP` from this document. Do not invent player-capacity or tick p95/p99 targets here.

## Phase 7.4 network observations (not SLA)

- Outbound path ownership in `network_pressure.json` schema 2 (`frames_*` / `bytes_*` / `write_calls`). `write_drain` is drain latency, not QUIC CPU.
- Peak outbound in tested envelope ≈ 0.53 MB/s (mixed@128). Transport cliff **beyond** mixed@128 / dense@64 on this host.
- See [`PHASE_74_REPORT.md`](PHASE_74_REPORT.md).

## Phase 7.5 CPU ownership observations (not SLA)

- Instant span close: post-push commit + outer publish glue attributed to existing leaves; unattributed share ~30–39% → ~15–25%.
- Growth owner: `replication_policy` (observer × pending). Peak tick util ≲11% through mixed@128 / dense@64. Parallelism **not** justified.
- See [`PHASE_75_REPORT.md`](PHASE_75_REPORT.md).

## Phase 7.6 capacity model (measured vs modeled)

- **Measured:** 7.5 ladder/canonical envelopes; falsify cells mixed@48, mixed@16+npc96, mixed@32 a4/p6, dense@48.
- **Modeled (not SLA):** `policy_ms ≈ 0.044 + 3.26e-4·scanned/tick`; `npc_ms ≈ -0.040 + 9.0e-3·npc_updates/tick`; tick holdout MAPE ~22%.
- **Sensitivity:** +10% players ≈ +6.7% tick at mixed@64 shape (fastest headroom consumer).
- **Memory model:** `rss_mb ≈ 9.54 + 0.096·P + 0.015·NPC` (calib MAPE ~1.6%).
- No production player cap. Far util≈50% extrapolations are unsupported.
- See [`PHASE_76_REPORT.md`](PHASE_76_REPORT.md).

## Phase 7.7 scaling architecture (decision; not SLA)

- **Canonical:** single process / single `World` owner (ADR-0056). No parallelism/sharding implemented.
- Likely first future limiter: `replication_policy`. Preferred first horizontal unit: channels/instances.
- Scaling triggers and readiness backlog: [`PHASE_77_REPORT.md`](PHASE_77_REPORT.md).

## Phase 7.8 production performance gate

- Entry: `./scripts/phase_78_gate.ps1` — thresholds in `scripts/phase_78_thresholds.json`.
- Frozen cells: mixed@8/64/128, dense@64, soak hotspot@32+NPC 8m; unit budget tests.
- Absolute tick/util caps preserve headroom vs 33.3 ms (not a player-capacity claim).
- Baseline: `logs/load/capacity_78/baseline/baseline.json`. Closed verdict **YELLOW** (harness WARN).
- See [`PHASE_78_REPORT.md`](PHASE_78_REPORT.md). **Phase 7 complete.**

## Phase 8C equipment wire sizes (measured codec; not compact ContentId mapping)

Control envelopes include the 1-byte tag:

| Payload | Bytes with tag |
|---|---:|
| EquipRequest | 14 (`seq` u32 + slot u8 + ContentId u64) |
| UnequipRequest | 6 |
| Equipment Accepted | 5 |
| Equipment Rejected | 6 |

Full equipment domain (occupied-mask + tokens; no Enter presence byte):

| State | Bytes |
|---|---:|
| All empty | 1 |
| One slot occupied | 9 |
| Six slots occupied | 49 |

One-slot deltas: equip **10**, unequip **2**. ContentId (8 bytes) dominates occupied payloads; compact mapping is deferred. Stable equipment after convergence emits **zero** equipment Update records until the next mutation. See [`PHASE_8C_REPORT.md`](PHASE_8C_REPORT.md).

## Phase 8D presentation bridge cost (not a capacity claim)

Per visible player: Humanoid v0 `copy_bind` + `evaluate` O(16). Pose buffers allocated on AOI enter and reused. `BoneTarget` → `BoneIndex` is bound once per `CharacterPresentationSet`, not per frame. Sync copies a small `Vec` of Copy presentation states. Local S2 debug evaluate is still separate this slice. See [`PHASE_8D_REPORT.md`](PHASE_8D_REPORT.md).
