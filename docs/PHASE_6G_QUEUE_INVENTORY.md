# Phase 6G queue and bounds inventory

Inventory **before** adding caps. Do not invent a max solely to look complete.

Untrusted/runtime input that can grow indefinitely should be bounded. Otherwise document why the lifecycle is inherently bounded.

| Structure | Producer | Consumer | Client reachable? | Cleanup | Growth | Existing backpressure | 6G action |
|---|---|---|---|---|---|---|---|
| Scheduler slots | `schedule_at` / effects / spawn / load-validation | drain per tick | No (server/sim) | cancel, fire, owner despawn | Rejects at `SCHEDULER_CAPACITY` 4096 | yes | keep |
| Critical drain | due Critical jobs | one pass / tick | No | remainder carries | Ceiling 1024 is overload safeguard, not a budget | yes | keep; tests do not lower it |
| Deferred drain | due Deferred jobs | budget 32 / tick | No | FIFO progress | Bounded by live scheduler slots | yes | keep |
| Session `InputCommand` queue | validated network commands | `take_for_tick` | Yes (untrusted) | epoch bump, HeldCancel, detach | `SESSION_QUEUE_CAP` 128 → Overflow | yes | keep |
| Lifecycle mpsc | handshake tasks | `GameplayOwner::drain` | Indirect | disconnect | `lifecycle_cap` from admission | channel cap | keep |
| Input handoff mpsc | connection tasks | sim drain | Yes | disconnect | `input_cap` from admission | channel cap | keep |
| Persist save mpsc | sim `try_save` | persist worker | No | shutdown drain | channel 64; `try_send` can drop | yes | keep; shutdown drain tested in **temp dir** |
| Replication writer queue | sim publish | async writer | No (derived) | detach | `WRITER_QUEUE_CAP` 4; sim does not encode-and-drop | yes | keep |
| Observer pending Enter/Update | AOI / DomainRevs | frame encode | No | Leave / epoch | per-observer mailbox + cadence | existing 6D policy | keep |
| `EventQueue` pending | runtime facts this tick | `commit` once | No | commit moves to processed | Per-tick producers; next-buffer holds same-tick pushes | lifecycle-bounded by tick work | **no new cap** |
| Action table | `try_start_action` | end / owner loss | Indirect (gated commands) | one Active per owner; despawn drops | owners ≤ live entities | Busy deny | **no new cap** |
| Effect table | `apply_test_effect` / future gameplay | expire / remove / despawn | No today | scheduler ExpireEffect | expires; despawn drops target | scheduler capacity | **no new cap** |
| Spawn schedule | `schedule_spawn` | SpawnDue | No | take on fire / cancel | scheduler-backed | scheduler capacity | **no new cap** |
| Cadence table | `register_cadence` | `pump_cadence` | No | none (World-scoped) | grows only when caller registers | load-validation registers finite N once | **no new cap**; do not register per tick |
| Occupancy map | attach/enter | detach | Indirect (login) | detach / drop lease | one entry per live Character | duplicate login rejected | **no new cap** |
| `LoadPressure` synthetic ids | load-validation arm | refill / despawn | No | process lifetime | clamped counts (malformed JSON safety, not quality thresholds) | load-mode gate | keep clamps as safety only |

## Intentionally not capped this phase

- `EventQueue` — drained every tick; unbounded growth would require unbounded same-tick producers, which are themselves scheduler/action/effect bounded or load-clamped.
- Action/Effect/Spawn tables — keyed to live entities or scheduler jobs; despawn/expire is the cleanup path.
- Cadence bindings — not entity-owned; callers must not register unbounded keys. Load-validation registers once.

No arbitrary quality-audit caps were added.
