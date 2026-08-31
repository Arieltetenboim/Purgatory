# Performance budgets

Living budgets. Values may be `TBD` until a phase produces measurements.

The 30 Hz / ~33.33 ms figures below are **tick spacing**, not permission for simulation code to consume 33 ms of CPU.

| Metric | Budget | Current | Notes |
|---|---|---|---|
| Server simulation tick rate | 30 Hz | 30 Hz configured | Initial engineering value, not a permanent promise. Constant: `TICK_RATE_HZ`. |
| Wall interval per simulation tick | ≈ 33.33 ms | 33_333_333 ns | `1_000_000_000 / 30` ns. Spacing between ticks, not a CPU allowance. |
| CPU time per simulation tick | TBD | TBD | Profile p50/p95/p99 before setting. Must stay well below the 33.33 ms wall interval. |
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
| Bytes/sec/player | TBD | 6D B N=50 mixed: ~20 KiB/s/client app payload | Characterization only; see [`PHASE_6D_PERFORMANCE.md`](PHASE_6D_PERFORMANCE.md). Not a bandwidth target or capacity claim |
| Snapshot entity decode bound | 256 | `MAX_ENTITIES_PER_SNAPSHOT` | Mechanical bound; byte cap still 8192 |
| Replication frame soft budget | 4096 bytes | `REPLICATION_FRAME_BUDGET_BYTES` | Pre-commit encoded size; wall remains `MAX_GAMEPLAY_SNAPSHOT_BYTES` 8192 |
| Writer queue cap | 4 frames | `WRITER_QUEUE_CAP` | If full, intent stays pending; no encode-and-drop |
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
| Load metrics export | localhost UDP | `127.0.0.1:5002` | Off-protocol `LoadMetricsV1` schema 3; missed poll ≠ zeros; rates from counter deltas. Schema 4 not added in 6G. |
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

## Recording rules

- Write measurements when a phase affects scale.
- Include machine specification, build mode, scenario, and observed bottleneck.
- Do not claim MMO scale from an unmeasured prototype.
- Database latency must not stall the simulation tick.
- Do not treat the tick interval as a CPU budget.

Open empirical issue, **not** a budget and **not** resolved: previously observed load-only stall around the 128-client case. Recorded for the upcoming capacity phase. Client `pending_window_stall_ticks` counts skipped predicted ticks when the 128-command pending window is full. Do not raise that cap from a budget document.
