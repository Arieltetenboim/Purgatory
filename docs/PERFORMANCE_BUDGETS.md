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
| Bytes/sec/player | TBD | control + 1 Hz datagram ping | No gameplay snapshots in Phase 5.0 |
| Connected players | TBD | ≥2 network sessions tested | Session identity only; no remote players rendered |
| Handshake timeout | 5 s | `HANDSHAKE_TIMEOUT` | Close if no Hello |
| Control message max | 4096 bytes | `MAX_CONTROL_MESSAGE_BYTES` | Length prefix checked before alloc |
| Ping cadence | 1 s | `PING_INTERVAL` | Datagram nonce only; not a gameplay timer |
| Active entities | TBD | 1,000 / 10,000 lifecycle tests | Debug, Windows MSVC, 2026-08-26: 1,000 tiny platforms spawn ≈ 86 µs, iterate ≈ 27 µs, despawn ≈ 58 µs. Not a player-count claim. Do not optimize from Debug timing. |
| Memory | TBD | TBD | Server and client reported separately |
| CPU | TBD | TBD | Include machine spec with every measurement |

## Recording rules

- Write measurements when a phase affects scale.
- Include machine specification, build mode, scenario, and observed bottleneck.
- Do not claim MMO scale from an unmeasured prototype.
- Database latency must not stall the simulation tick.
- Do not treat the tick interval as a CPU budget.
