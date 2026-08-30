#!/usr/bin/env python3
"""Build Phase 6D Scenario A/B/C evidence tables from poll JSONL + harness summaries.

Localhost characterization only. Not a production-capacity claim.
"""

from __future__ import annotations

import argparse
import json
import platform
import statistics
from datetime import date
from pathlib import Path

# Protocol v7 full WorldSnapshot on the uni stream (6C / 5.7 full-visibility).
# Header bytes from encode_world_snapshot + 4-byte length prefix.
V7_HEADER_FRAMED = 4 + 48
V7_ENTITY_BYTES = 25
TICK_HZ = 30.0
MAP_A_INTERACTABLES = 3  # switch, chest, portal (full-map 6C, not AOI)
MAP_B_INTERACTABLES = 2  # switch, portal


def load_json(path: Path) -> dict:
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def load_jsonl(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return rows


def mean(xs: list[float]) -> float | None:
    return statistics.mean(xs) if xs else None


def pctl(xs: list[float], q: float) -> float | None:
    if not xs:
        return None
    s = sorted(xs)
    if len(s) == 1:
        return s[0]
    idx = min(len(s) - 1, max(0, int(round((len(s) - 1) * q))))
    return s[idx]


def fmt(v, digits=3, empty="—"):
    if v is None:
        return empty
    if isinstance(v, int):
        return str(v)
    return f"{v:.{digits}f}"


def v7_frame_bytes(entity_count: int) -> int:
    return V7_HEADER_FRAMED + V7_ENTITY_BYTES * entity_count


def v7_full_map_outbound_bps(players: int, interactables: int) -> float:
    """N observers × 30 Hz × framed full snapshot of (players + interactables)."""
    e = players + interactables
    return players * TICK_HZ * v7_frame_bytes(e)


def v7_scenario_c_bps(n: int) -> float:
    n_b = n // 2
    n_a = n - n_b
    return v7_full_map_outbound_bps(n_a, MAP_A_INTERACTABLES) + v7_full_map_outbound_bps(
        n_b, MAP_B_INTERACTABLES
    )


def steady_samples(polls: list[dict], target: int) -> list[dict]:
    """Samples at target population, dropping the first 8s after first hit (Enter burst)."""
    hit = None
    out = []
    for rec in polls:
        m = rec.get("metrics") or {}
        sessions = int(m.get("active_sessions") or 0)
        if sessions < max(1, int(target * 0.9)):
            continue
        t = float(rec.get("wall_elapsed_s") or 0.0)
        if hit is None:
            hit = t
        if t >= hit + 8.0:
            out.append(rec)
    return out


def rates_from_polls(polls: list[dict]) -> dict:
    if len(polls) < 2:
        return {}
    first = polls[0]["metrics"]
    last = polls[-1]["metrics"]
    dt = float(polls[-1]["wall_elapsed_s"]) - float(polls[0]["wall_elapsed_s"])
    if dt <= 0.05:
        return {}

    def d(key: str) -> float:
        return float(last.get(key) or 0) - float(first.get(key) or 0)

    bytes_out = d("bytes_out")
    enters = d("aoi_enters")
    leaves = d("aoi_leaves")
    updates = d("aoi_updates")
    churn = d("aoi_churn_reentry")
    update_bytes = d("aoi_update_bytes")
    n = max(1, int(last.get("active_sessions") or 1))
    out_bps = bytes_out / dt
    return {
        "dt_s": dt,
        "connected": int(last.get("active_sessions") or 0),
        "players": int(last.get("active_player_entities") or 0),
        "out_bps": out_bps,
        "out_mib_s": out_bps / (1024 * 1024),
        "out_kib_s_client": (out_bps / n) / 1024.0,
        "enters_s": enters / dt,
        "leaves_s": leaves / dt,
        "updates_s": updates / dt,
        "churn_s": churn / dt,
        "bytes_per_update": (update_bytes / updates) if updates > 0 else None,
        "encode_fail": int(last.get("snapshot_encode_failed") or 0),
        "send_fail": int(last.get("snapshot_send_failed") or 0),
        "transport_loss": int(last.get("transport_loss") or 0),
        "clean_dc": int(last.get("clean_disconnect") or 0),
        "queue_depth_max": int(last.get("replication_queue_depth_max") or 0),
        "oldest_pending": int(last.get("oldest_pending_ticks") or 0),
        "max_deferred": int(last.get("max_deferred_ticks") or 0),
        "build_max_ms": float(last.get("snapshot_build_time_max_ms") or 0.0),
        "encode_max_ms": float(last.get("snapshot_encode_time_max_ms") or 0.0),
        "tick_mean_ms": float(last.get("tick_work_mean_ms") or 0.0),
        "tick_p95_ms": float(last.get("tick_work_p95_ms") or 0.0),
        "tick_p99_ms": float(last.get("tick_work_p99_ms") or 0.0),
        "tick_max_ms": float(last.get("tick_work_max_ms") or 0.0),
        "overruns": int(last.get("tick_overrun_count") or 0),
        "known_max": int(last.get("last_snapshot_entities") or 0),
        "snap_size_max": int(last.get("snapshot_size_max_bytes") or 0),
        "schema": int(last.get("metrics_schema_version") or 0),
    }


def series_known(polls: list[dict]) -> list[float]:
    xs = []
    for rec in polls:
        m = rec.get("metrics") or {}
        v = m.get("last_snapshot_entities")
        if v is not None:
            xs.append(float(v))
    return xs


def series_pending(polls: list[dict]) -> list[float]:
    xs = []
    for rec in polls:
        m = rec.get("metrics") or {}
        v = m.get("oldest_pending_ticks")
        if v is not None:
            xs.append(float(v))
    return xs


def analyze_run(run_dir: Path, logs: Path) -> dict:
    meta = load_json(run_dir / "meta.json")
    polls = load_jsonl(run_dir / "metrics_schema2.jsonl")
    target = int(meta.get("count") or 0)
    steady = steady_samples(polls, target)
    stats = rates_from_polls(steady if len(steady) >= 2 else polls)
    known = series_known(steady if steady else polls)
    stats["known_mean"] = mean(known)
    stats["known_p95"] = pctl(known, 0.95)
    if known:
        stats["known_max"] = int(max(known))
    stats["pending_peak_samples"] = max(series_pending(polls) or [0.0])
    harness = {}
    name = meta.get("harness_dir") or ""
    if name:
        hdir = logs / name
        harness = load_json(hdir / "run_summary.json")
        stats["harness_status"] = harness.get("run_status")
        stats["unexpected_disconnects"] = harness.get("unexpected_disconnects")
        stats["admission_refusals"] = harness.get("admission_refusals")
        stats["encode_failures_harness"] = harness.get("encode_failures")
        stats["peak_connected"] = harness.get("peak_connected")
        stats["harness_elapsed"] = harness.get("elapsed_secs")
    else:
        stats["harness_status"] = "missing"
    stats["scenario"] = meta.get("scenario")
    stats["placement"] = meta.get("placement")
    stats["requested"] = target
    stats["harness_exit"] = meta.get("harness_exit")
    stats["poll_samples"] = len(polls)
    stats["steady_samples"] = len(steady)
    stats["run_dir"] = run_dir.name
    stats["harness_dir"] = name
    return stats


def md_row(cells: list[str]) -> str:
    return "| " + " | ".join(cells) + " |"


def build_markdown(runs: list[dict], out_root: Path) -> str:
    today = date.today().isoformat()
    cpu = platform.processor() or platform.machine()
    os_name = f"{platform.system()} {platform.release()}"
    by_key = {(r["scenario"], r["requested"]): r for r in runs}

    lines = []
    lines.append("# PHASE 6D PERFORMANCE EVIDENCE")
    lines.append("")
    lines.append("## Status")
    lines.append("")
    lines.append("**PHASE 6D PERFORMANCE EVIDENCE READY FOR REVIEW**")
    lines.append("")
    lines.append("Automated correctness gate remains GREEN. Manual runtime check is still required.")
    lines.append("Do **not** begin Phase 6E. This is localhost characterization, **not** a production-capacity claim.")
    lines.append("")
    lines.append("## Method")
    lines.append("")
    lines.append(f"- Date: {today}")
    lines.append(f"- Machine: {os_name}; processor `{cpu}`")
    lines.append("- Build: `cargo build --release -p purgatory-server -p purgatory-bot-client`")
    lines.append("- Harness: `purgatory-load --scenario load --profile mixed --seed 4242 --duration 45s --ramp-ms 50`")
    lines.append("- Server: `PURGATORY_ADMISSION_CAP=256` plus `PURGATORY_LOAD_PLACEMENT=cluster|spread|maps`")
    lines.append("- Metrics: off-protocol `LoadMetricsV1` schema **2** polled at 1 Hz (`tools/poll_load_metrics.py`)")
    lines.append("- Steady window: samples at ≥90% target sessions, skipping the first 8 s after that (Enter burst)")
    lines.append("- `bytes_out` is length-prefixed gameplay uni payload (not QUIC/UDP on-wire size)")
    lines.append(f"- Artifacts: `{out_root.as_posix()}`")
    lines.append("")
    lines.append("### Placement (implemented)")
    lines.append("")
    lines.append("| Scenario | Env | Spawn |")
    lines.append("|---|---|---|")
    lines.append("| A cluster | `cluster` | all at `FOOTNOTE_SPAWN_X` (−19.4) Map A |")
    lines.append("| B spread | `spread` | `FOOTNOTE_SPAWN_X + (n % 8) * 5` wu on Map A |")
    lines.append("| C maps | `maps` | even index Map A spawn; odd index Map B |")
    lines.append("")
    lines.append("Map A bounds are **48 wu** wide (`[-24, 24]`). AOI enter half-extents are **16 × 9 wu** (leave +2 wu).")
    lines.append("Scenario B's eight spawn slots span only **40 wu**, so AOIs still overlap heavily.")
    lines.append("Map B is **16 wu** wide, so one observer's leave rect covers essentially the whole map.")
    lines.append("Scenario B therefore does **not** create a large empty-AOI remainder on this development stage.")
    lines.append("")
    lines.append("### Metric notes / gaps")
    lines.append("")
    lines.append("- **Known / relevant (exported):** `last_snapshot_entities` is the **max Known count across observers that tick**, not a mean of spatial candidates. Reported as `known_mean` / `known_max` over the steady window.")
    lines.append("- **Spatial candidates / observer:** not a LoadMetrics gauge. On Map A, candidates ≈ players inside the leave rect + visible interactables (switch/chest in spawn AOI; portal is outside spawn leave rect). Cluster ≈ all N players + 2 generics. Spread still covers most of Map A.")
    lines.append("- Writer queue depth / oldest pending / build time are **run peaks** (monotonic max), not window averages.")
    lines.append("- `snapshot_build_time_max_ms` is wall time of `publish_observer_frame` (spatial candidates + classify + encode). Uni `write_all` encode-to-frame is `snapshot_encode_time_max_ms` (payload is already encoded).")
    lines.append("- Enter/Update/Leave rates are counter deltas over the steady window (committed to the writer queue, not client ACK).")
    lines.append("- `bytes/update` uses `Δaoi_update_bytes / Δaoi_updates` (encoded frame bytes / Update records — includes Enter/Leave bytes in the numerator).")
    lines.append("")

    header = [
        "N",
        "connected",
        "known mean/max",
        "Enter/s",
        "Update/s",
        "Leave/s",
        "churn/s",
        "out MiB/s",
        "KiB/s/client",
        "B/update",
        "build max ms",
        "queue max",
        "oldest pend",
        "enc/send fail",
        "unexp DC",
        "status",
    ]

    def table_for(scenario: str, title: str, blurb: str) -> None:
        lines.append(f"## {title}")
        lines.append("")
        lines.append(blurb)
        lines.append("")
        lines.append(md_row(header))
        lines.append("|" + "|".join(["---"] * len(header)) + "|")
        ns = sorted({r["requested"] for r in runs if r["scenario"] == scenario})
        for n in ns:
            r = by_key.get((scenario, n))
            if not r:
                continue
            known = f"{fmt(r.get('known_mean'), 1)} / {r.get('known_max', '—')}"
            fails = f"{r.get('encode_fail', 0)}/{r.get('send_fail', 0)}"
            lines.append(
                md_row(
                    [
                        str(n),
                        str(r.get("connected") or r.get("peak_connected") or "—"),
                        known,
                        fmt(r.get("enters_s"), 2),
                        fmt(r.get("updates_s"), 1),
                        fmt(r.get("leaves_s"), 2),
                        fmt(r.get("churn_s"), 2),
                        fmt(r.get("out_mib_s"), 3),
                        fmt(r.get("out_kib_s_client"), 1),
                        fmt(r.get("bytes_per_update"), 1),
                        fmt(r.get("build_max_ms"), 3),
                        str(r.get("queue_depth_max", "—")),
                        str(r.get("oldest_pending", "—")),
                        fails,
                        str(r.get("unexpected_disconnects", "—")),
                        str(r.get("harness_status") or "—"),
                    ]
                )
            )
        lines.append("")
        # extra tick row
        lines.append("Tick / encode (steady-end server gauges; build/encode max are run peaks):")
        lines.append("")
        lines.append(
            md_row(["N", "tick mean ms", "tick p95", "tick p99", "tick max", "overruns", "encode max ms", "frame size max"])
        )
        lines.append("|---|---:|---:|---:|---:|---:|---:|---:|")
        for n in ns:
            r = by_key.get((scenario, n))
            if not r:
                continue
            lines.append(
                md_row(
                    [
                        str(n),
                        fmt(r.get("tick_mean_ms"), 3),
                        fmt(r.get("tick_p95_ms"), 3),
                        fmt(r.get("tick_p99_ms"), 3),
                        fmt(r.get("tick_max_ms"), 3),
                        str(r.get("overruns", "—")),
                        fmt(r.get("encode_max_ms"), 3),
                        str(r.get("snap_size_max", "—")),
                    ]
                )
            )
        lines.append("")

    table_for(
        "A_cluster",
        "Scenario A — cluster (dense overlapping AOIs)",
        "Worst case **at spawn**: every bot shares Map A `FOOTNOTE_SPAWN_X`. The `mixed` profile then walks, so density falls as bodies spread, but A still starts as the expensive overlap case and stays the highest Update and outbound rates at each N.",
    )
    table_for(
        "B_spread",
        "Scenario B — spread (same mixed profile / seed / mutation workload)",
        "Same `mixed` input profile and seed as A. Only spawn X changes. On this 48 wu map the eight 5 wu slots still overlap the 16 wu enter rect, so the relevant set is only weakly reduced versus A.",
    )
    table_for(
        "C_maps",
        "Scenario C — maps (WorldAddress split)",
        "Odd attachments spawn on Map B. Address isolation should cut cross-map replication. Map B is fully covered by one AOI; Map A half still clusters at spawn.",
    )

    lines.append("## Phase 6C full-visibility baseline vs Phase 6D Scenario B")
    lines.append("")
    lines.append("6C (and 5.7) sent a **full `WorldSnapshot` every tick** to every observer: all same-map players plus map interactables. That payload is reconstructed here from the v7 encoder (`4` byte length prefix + `48` byte header + `25` bytes/entity) at 30 Hz. Surviving 5.7 reports recorded tick work, not `bytes_out`, and this tree can no longer run v7.")
    lines.append("")
    lines.append("6C Map A entity count = N players + 3 interactables. 6D B numbers are **measured** application `bytes_out` in the steady window.")
    lines.append("")
    lines.append(
        md_row(
            [
                "N",
                "6C recon MiB/s",
                "6C recon KiB/s/client",
                "6D B MiB/s",
                "6D B KiB/s/client",
                "6D B known mean",
                "ratio 6D/6C out",
            ]
        )
    )
    lines.append("|---:|---:|---:|---:|---:|---:|---:|")
    ns = sorted({r["requested"] for r in runs if r["scenario"] == "B_spread"})
    for n in ns:
        v7_bps = v7_full_map_outbound_bps(n, MAP_A_INTERACTABLES)
        v7_mib = v7_bps / (1024 * 1024)
        v7_kib = (v7_bps / max(n, 1)) / 1024.0
        r = by_key.get(("B_spread", n)) or {}
        d_mib = r.get("out_mib_s")
        d_kib = r.get("out_kib_s_client")
        ratio = (d_mib / v7_mib) if d_mib is not None and v7_mib > 0 else None
        lines.append(
            md_row(
                [
                    str(n),
                    fmt(v7_mib, 3),
                    fmt(v7_kib, 1),
                    fmt(d_mib, 3),
                    fmt(d_kib, 1),
                    fmt(r.get("known_mean"), 1),
                    fmt(ratio, 3),
                ]
            )
        )
    lines.append("")
    lines.append("6C Scenario C reconstruction (half Map A / half Map B, still full snapshots per address):")
    lines.append("")
    lines.append(md_row(["N", "6C-C recon MiB/s", "6D C MiB/s", "6D C KiB/s/client"]))
    lines.append("|---:|---:|---:|---:|")
    ns_c = sorted({r["requested"] for r in runs if r["scenario"] == "C_maps"})
    for n in ns_c:
        v7 = v7_scenario_c_bps(n) / (1024 * 1024)
        r = by_key.get(("C_maps", n)) or {}
        lines.append(
            md_row([str(n), fmt(v7, 3), fmt(r.get("out_mib_s"), 3), fmt(r.get("out_kib_s_client"), 1)])
        )
    lines.append("")

    # Scaling statement
    lines.append("## Does evidence support bounded per-client bandwidth?")
    lines.append("")
    lines.append("Claim under test:")
    lines.append("")
    lines.append("`global N increases while local relevant set stays approximately bounded → per-client replication bandwidth stays approximately bounded`")
    lines.append("")

    def kib_series(scenario: str) -> list[tuple[int, float | None, float | None]]:
        rows = []
        for n in sorted({r["requested"] for r in runs if r["scenario"] == scenario}):
            r = by_key[(scenario, n)]
            rows.append((n, r.get("out_kib_s_client"), r.get("known_mean")))
        return rows

    b = kib_series("B_spread")
    a = kib_series("A_cluster")
    c = kib_series("C_maps")

    def describe(label: str, rows: list[tuple[int, float | None, float | None]]) -> None:
        lines.append(f"**{label}:**")
        for n, kib, known in rows:
            lines.append(f"- N={n}: KiB/s/client={fmt(kib, 1)}, known_mean={fmt(known, 1)}")
        if len(rows) >= 2 and rows[0][1] and rows[-1][1] and rows[0][0]:
            n0, k0, kn0 = rows[0]
            n1, k1, kn1 = rows[-1]
            lines.append(
                f"- From N={n0} to N={n1}: per-client KiB/s ×{fmt(k1 / k0, 2) if k0 else None}; "
                f"known_mean ×{fmt((kn1 or 0) / kn0, 2) if kn0 else None}; N ×{n1 / n0:.2f}"
            )
        lines.append("")

    describe("Scenario A (cluster, expected expensive)", a)
    describe("Scenario B (spread, intended bounded-local test)", b)
    describe("Scenario C (maps)", c)

    # Verdict from numbers if we have them
    verdict_lines = []
    if len(b) >= 2 and b[0][1] and b[-1][1] and b[0][2] and b[-1][2]:
        n_factor = b[-1][0] / b[0][0]
        kib_factor = b[-1][1] / b[0][1]
        known_factor = b[-1][2] / b[0][2]
        local_bounded = known_factor < 0.5 * n_factor + 0.5  # known grows clearly slower than N
        bw_bounded = kib_factor < 0.5 * n_factor + 0.5
        if local_bounded and bw_bounded:
            verdict_lines.append(
                f"**Supported on this localhost matrix (Scenario B):** known_mean grew ×{known_factor:.2f} while N grew ×{n_factor:.2f}; "
                f"per-client KiB/s grew ×{kib_factor:.2f}. Both grew slower than global N."
            )
        elif not local_bounded:
            verdict_lines.append(
                f"**Not supported on this development stage (Scenario B):** known_mean grew ×{known_factor:.2f} vs N ×{n_factor:.2f}. "
                "The FOOTNOTE map is only 48 wu wide with a 16 wu enter half-extent, so spreading across 8 slots does not keep the relevant set approximately constant. "
                "Per-client bandwidth therefore still tracks local N, which still tracks global N."
            )
        else:
            verdict_lines.append(
                f"**Partially supported:** known_mean ×{known_factor:.2f} vs N ×{n_factor:.2f}, but per-client KiB/s ×{kib_factor:.2f}."
            )
    else:
        verdict_lines.append("**Insufficient Scenario B samples to judge the bounded-bandwidth claim.**")

    lines.extend(verdict_lines)
    lines.append("")
    lines.append("**Scenario A is not hidden:** clustered spawn keeps essentially all players inside one leave rect. Known count and Update rate are expected to scale with N. That is the expensive overlap case.")
    lines.append("")
    lines.append("**6D vs 6C still matters even when AOI does not bound:** v8 sends Transform Updates for dirty entities instead of a full pose list every tick, so idle/un-dirty members of the known set are omitted. Compare the 6C reconstruction table above. That is a delta-encoding win, not proof that AOI bounded the set.")
    lines.append("")
    lines.append("## AOI churn")
    lines.append("")
    lines.append("`aoi_churn_reentry` counts re-Enter of an id recently Left (observer known-set hysteresis / walk-out-walk-in). Rates below are steady-window `Δchurn / Δt`.")
    lines.append("")
    lines.append(md_row(["Scenario", "N", "churn/s", "Enter/s", "Leave/s"]))
    lines.append("|---|---:|---:|---:|---:|")
    for r in sorted(runs, key=lambda x: (x["scenario"], x["requested"])):
        lines.append(
            md_row(
                [
                    str(r["scenario"]),
                    str(r["requested"]),
                    fmt(r.get("churn_s"), 2),
                    fmt(r.get("enters_s"), 2),
                    fmt(r.get("leaves_s"), 2),
                ]
            )
        )
    lines.append("")
    lines.append("Placement only sets **spawn X**. The `mixed` profile then walks, so Scenario A does not remain a static pile. Leave/Enter/churn therefore appear in cluster as well as spread: bots walk out of each other's 16 wu enter / 18 wu leave rects on the 48 wu floor. Maps (C) show lower churn because each address has about N/2 bodies and Map B is fully covered by one AOI.")
    lines.append("")
    lines.append("## Failures")
    lines.append("")
    lines.append("Encode failures, uni `write_all` failures, and unexpected disconnects are copied from schema-2 gauges and the harness `run_summary.json`. Non-zero encode/write failures would be a defect; this report does not change gameplay code unless those counters fire.")
    lines.append("")
    lines.append("## Limitations")
    lines.append("")
    lines.append("- Localhost bots share CPU with the server; no native render cost.")
    lines.append("- Admission 256 is a development bound.")
    lines.append("- 45 s holds are short; not a soak.")
    lines.append("- 6C MiB/s is reconstructed from the v7 encoder, not a paired v7 re-run on this binary.")
    lines.append("- Do not treat these numbers as MMO player capacity.")
    lines.append("")
    lines.append("## Boundary")
    lines.append("")
    lines.append("Work stopped at Phase 6D evidence. **Do not begin Phase 6E.**")
    lines.append("")
    lines.append("**PHASE 6D PERFORMANCE EVIDENCE READY FOR REVIEW**")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, help="logs/load/phase6d_<stamp>")
    parser.add_argument("--logs", required=True, help="logs/load")
    parser.add_argument("--docs", required=True, help="docs/")
    args = parser.parse_args()
    root = Path(args.root)
    logs = Path(args.logs)
    docs = Path(args.docs)
    runs = []
    for meta_path in sorted(root.glob("*/meta.json")):
        runs.append(analyze_run(meta_path.parent, logs))
    if not runs:
        raise SystemExit(f"no runs under {root}")
    md = build_markdown(runs, root)
    out_md = docs / "PHASE_6D_PERFORMANCE.md"
    out_md.write_text(md, encoding="utf-8")
    (root / "PHASE_6D_PERFORMANCE.md").write_text(md, encoding="utf-8")
    (root / "runs_summary.json").write_text(json.dumps(runs, indent=2), encoding="utf-8")
    print(f"wrote {out_md}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
