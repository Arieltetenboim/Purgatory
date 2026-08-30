# Phase 5.6 — latency matrix (manual)

Fill during two-client local runs. Do **not** invent numbers. Do **not** retune prediction or interpolation to improve this table.

All configured delays are **one-way artificial delay**, not measured ping. Ping/Pong RTT in the Network tab stays the unimpaired datagram path.

**Fidelity:** snapshot impairment is client application-delivery delay after a successful uni-stream read. It is **not** server QUIC send-buffer / flow-control stall.

**Correction** = pre-reconcile predicted pose → post restore+replay predicted pose. **Lead** = auth → replayed predicted with pending. **Aligned residual** is diagnostic only — do not put it in the correction column.

**Observed ack delta** is client-observed ack advancement between accepted snapshots. It is **not** late-collapse. True `late_collapse_count` is server-side (tests / verbose logs).

**Interpolation:** the presentation buffer is still 3 ticks (~100 ms). Underrun / discontinuity beyond that buffer is **measured here**, not an automatic Phase 5.6 correctness failure.

| Profile / delay | Pending typical / max | Correction typical / max (wu) | Lead typical / max | Observed ack delta typical / max | Hard snaps | Interp quality (holds / snaps / notes) | Gameplay issue |
|---|---|---|---|---|---|---|---|
| Clean (0 ms) | | | | | | | |
| ~50 ms one-way | | | | | | | |
| ~100 ms one-way | | | | | | | |
| ~150 ms one-way | | | | | | | |
| ~250 ms one-way | | | | | | | |
| Moderate + jitter | | | | | | | |
| Stall 250 ms | | | | | | | |
| Stall 500 ms | | | | | | | |
| Stall 1000 ms | | | | | | | |

## Checklist

Baseline (Clean): short 5.5 movement test, no regression.

Fixed delay (50 / 100 / 150 / 250 ms one-way): sustained move, direction changes, jump, run+jump, platform edge, OneWay, drop-through.

Jitter (Moderate): watch pending, ack, correction, interpolation.

Stall (250 / 500 / 1000 ms) while standing, running, airborne, immediately before jump. After recovery: ack advances, pending returns downward, continuation debt clears, late-collapse occurs where expected (server), no permanent input-latency backlog.

Legitimate under delay (not a 5.5 failure): predicted jump while grounded, stall, server continues, late jump after unground → authoritative jump does not occur → restore+replay corrects.

Failures: pending grows permanently after recovery; continuation debt never clears; ack stops advancing; ordinary movement needs repeated hard snaps; epoch/sequence invariant breaks; client stays permanently displaced after recovery.

## Env

```text
PURGATORY_NET_IMPAIRMENT=off|clean|lan|moderate|bad|stalltest
PURGATORY_NET_IMPAIRMENT_SEED=<u64>
```

Overlay (Backquote, Network tab): profile selector, Stall input 250/500/1000 ms, Reset impairment metrics (explicit; profile change does not reset).
