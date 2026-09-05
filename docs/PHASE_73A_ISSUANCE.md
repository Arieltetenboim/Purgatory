# Phase 7.3A — Harness issuance fix (partial report)

**Status:** issuance proof **PASS** (2026-09-01). Capacity ladders gated on.

## Root cause (confirmed)

Controller issued at most one `queue_bot_connect` per tick **after** serial `tick_all_bots` (1 ms stream polls × N). Missed ticks never caught up → ~1.9 Hz issuance (7.1F: 114/384).

## Fix (harness only)

In [`tools/bot_client/src/controller.rs`](../tools/bot_client/src/controller.rs):

1. Issue due connects **before** bot I/O.
2. **Catch-up** from ramp schedule (`next_due += ramp_ms`), not wall `now`.
3. **Thin rotate poll** (cap 8, drain 1) while `!ramp_complete`; full poll after.
4. Parallel `JoinSet` connects unchanged.
5. Metrics on `connection_ramp.json`: controller tick / `tick_all_bots` p50/p99, `spawn_catchup_issued_total`, `spawn_due_peak`.

No admission / QUIC / prediction / replication / gameplay changes.

## Proof (`scripts/capacity_73a_issuance_proof.ps1`)

Artifacts: `logs/load/capacity_73/summary_73a_20260901_211904/phase73a_issuance_proof.json`

| Cell | requested | spawn_issued | peak_active | admission_refused | issue_ratio | funnel_invariant |
|---|---|---|---|---|---|---|
| hotspot@128 / 45s | 128 | **128** | 128 | 0 | 1.0 | true |
| hotspot@384 / 60s (admit 256) | 384 | **384** | **256** | **128** | 1.0 | true |

Ownership @384: peak active hit admission cap with refusals — **not** harness issuance shortfall.

Catch-up fired (`spawn_catchup` 106 / 302; `spawn_due_peak` 10).

## Non-issuance note

Load process exit was **FAILED** on @128 for `snapshot_starvation` (post-ramp full O(N)×1 ms poll cadence ~1.2 s/tick). That is **not** an issuance miss; 7.3A gate keys on issue_ratio + funnel invariant. Post-ramp poll cost may still affect high-N gameplay fidelity / classify — measure in ladders; do not treat as server capacity.

## Gate to ladders

1. Requested issuance attained (100%).
2. Funnel invariant passes (schema 2).
3. Harness controller no longer first issuance bottleneck.
4. Server admission wall distinguishable from harness shortfall.

**Proceed to single-axis 7.3 ladders** (measure / attribute only; no server optimization).
