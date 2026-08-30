#!/usr/bin/env python3
"""Build a Phase 5.7 capacity comparison from completed load-run directories.

Does not re-run load tests. Reads each run's run_summary.json + metrics.csv
and writes logs/load/capacity/<stamp>/{capacity_summary.json,md,graphs/}.
"""

from __future__ import annotations

import argparse
import csv
import json
import statistics
import sys
from datetime import datetime, timezone
from pathlib import Path


TICK_BUDGET_MS = 33.333
PERCENTILE_NOTE = (
    "p95*/p99* = peak of server <=120-tick window percentiles "
    "(tick_percentile_semantics=peak_of_window_percentiles), not full-run percentiles"
)


def f(v, default=None):
    if v is None or v == "":
        return default
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def load_json(path: Path) -> dict:
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def load_csv(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    with path.open(newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def col(rows: list[dict], *keys: str) -> list[float]:
    out = []
    for r in rows:
        for k in keys:
            raw = r.get(k, "")
            if raw is None or raw == "":
                continue
            try:
                out.append(float(raw))
                break
            except ValueError:
                continue
    return out


def classify_capacity(row: dict) -> str:
    """Localhost capacity class — not production capacity."""
    status = str(row.get("run_status", "")).upper()
    overruns = int(f(row.get("server_tick_overruns_total"), 0) or 0)
    disconnects = int(f(row.get("unexpected_disconnects"), 0) or 0)
    encode = int(f(row.get("encode_failures"), 0) or 0)
    overflow = int(f(row.get("overflow_events"), 0) or 0)
    # Prefer harness early overflow snapshot; final classify may use larger server counter.
    reasons = row.get("status_reasons") or []
    reason_codes = {
        (r.get("code") if isinstance(r, dict) else "") for r in reasons
    }
    starve = int(f(row.get("snapshot_starvation_samples"), 0) or 0)
    admission = int(f(row.get("admission_refusals"), 0) or 0)
    peak = int(f(row.get("peak_connected"), 0) or 0)
    requested = int(f(row.get("requested_bots"), 0) or 0)
    tick_max = f(row.get("server_tick_work_max_ms"), 0.0) or 0.0
    tick_mean = f(row.get("server_tick_work_mean_ms"), 0.0) or 0.0
    p99 = f(row.get("server_tick_work_p99_ms"), 0.0) or 0.0
    queue_in = int(f(row.get("server_input_queue_max"), 0) or 0)
    queue_sess = int(f(row.get("server_session_queue_max"), 0) or 0)

    hard_fail = (
        status == "FAILED"
        or overruns > 0
        or encode > 0
        or overflow > 0
        or "queue_overflow" in reason_codes
        or (starve >= 5)
        or (admission > 0 and peak < requested)
        or disconnects > max(1, requested // 50)
    )
    if hard_fail:
        return "FAILING"

    spike_ratio = (tick_max / tick_mean) if tick_mean > 1e-6 else 0.0
    if (
        admission > 0
        or p99 > 10.0
        or tick_max > 20.0
        or spike_ratio > 40.0
        or queue_sess >= 8
        or queue_in >= 8
        or disconnects > 0
        or status == "WARN"
    ):
        return "PRESSURED"

    return "HEALTHY"


def find_knee(rows: list[dict]) -> dict:
    """Heuristic localhost knee — correctness vs simulation vs admission wall."""
    correctness = None
    sim = None
    admission_wall = None
    for i, row in enumerate(rows):
        if (
            admission_wall is None
            and int(f(row.get("admission_refusals"), 0) or 0) > 0
            and row["peak_connected"] < row["requested_bots"]
        ):
            admission_wall = {
                "bots_connected": row["peak_connected"],
                "requested": row["requested_bots"],
                "reason": "load-mode admission/entity bound (256) refused further sessions",
                "run": row["run_name"],
            }
        if correctness is None and row["capacity_class"] == "FAILING":
            correctness = {
                "bots": row["peak_connected"],
                "requested": row["requested_bots"],
                "reason": "first FAILING class (overflow/disconnect/starvation/admission shortfall)",
                "run": row["run_name"],
            }
        if i > 0 and sim is None:
            prev = rows[i - 1]
            pop_ratio = (
                row["peak_connected"] / prev["peak_connected"]
                if prev["peak_connected"]
                else None
            )
            a = f(row.get("server_tick_work_mean_ms"))
            b = f(prev.get("server_tick_work_mean_ms"))
            out_a = f(row.get("outbound_bytes_per_sec_mean"))
            out_b = f(prev.get("outbound_bytes_per_sec_mean"))
            mean_ratio = (a / b) if a and b and b > 0 else None
            out_ratio = (out_a / out_b) if out_a and out_b and out_b > 0 else None
            if pop_ratio and mean_ratio and mean_ratio > pop_ratio * 1.35 and mean_ratio > 1.5:
                sim = {
                    "bots": row["peak_connected"],
                    "requested": row["requested_bots"],
                    "reason": (
                        f"tick mean grew {mean_ratio:.2f}x while population grew "
                        f"{pop_ratio:.2f}x"
                    ),
                    "run": row["run_name"],
                }
            elif pop_ratio and out_ratio and out_ratio > pop_ratio * 1.6:
                sim = {
                    "bots": row["peak_connected"],
                    "requested": row["requested_bots"],
                    "reason": (
                        f"outbound rate grew {out_ratio:.2f}x while population grew "
                        f"{pop_ratio:.2f}x (snapshot fan-out)"
                    ),
                    "run": row["run_name"],
                }

    # Primary knee for the report: earliest of correctness / clear sim pressure.
    primary = correctness or sim or admission_wall
    return {
        "localhost_knee": primary,
        "correctness_onset": correctness,
        "simulation_pressure_onset": sim,
        "admission_wall": admission_wall,
        "disclaimer": (
            "Localhost shared-CPU characterization only. "
            "Not a production capacity claim."
        ),
    }


def analyze_spikes(rows: list[dict]) -> dict:
    """Correlate large tick_max samples with ramp / snapshot / queue columns."""
    pairs = []
    for r in rows:
        tmax = f(r.get("server_tick_work_max_ms") or r.get("server_tick_max_ms"))
        if tmax is None:
            continue
        pairs.append(
            {
                "elapsed_secs": f(r.get("elapsed_secs"), 0.0),
                "tick_max_ms": tmax,
                "connected": f(r.get("connected_bots") or r.get("connected"), 0.0),
                "snap_build_max_ms": f(r.get("snapshot_build_time_max_ms")),
                "snap_encode_max_ms": f(r.get("snapshot_encode_time_max_ms")),
                "input_q": f(r.get("server_input_queue_current")),
                "session_q_max": f(r.get("server_session_queue_max")),
            }
        )
    if not pairs:
        return {"spike_count_over_10ms": 0, "notes": "no tick max series"}

    over = [p for p in pairs if p["tick_max_ms"] >= 10.0]
    over.sort(key=lambda p: p["tick_max_ms"], reverse=True)
    top = over[:5]
    during_ramp = sum(1 for p in over if (p["connected"] or 0) < (pairs[-1]["connected"] or 0) * 0.95)
    note = []
    if over:
        note.append(f"{len(over)} samples with window-max ≥ 10 ms")
        if during_ramp and during_ramp >= len(over) * 0.5:
            note.append("majority of large spikes occurred while population still ramping")
        else:
            note.append("large spikes also appear after ramp (steady population)")
        # Weak correlation hints
        with_snap = [
            p
            for p in over
            if (p["snap_build_max_ms"] or 0) > 1.0 or (p["snap_encode_max_ms"] or 0) > 1.0
        ]
        if with_snap:
            note.append(
                f"{len(with_snap)}/{len(over)} large spikes co-occur with elevated snapshot build/encode max"
            )
        else:
            note.append(
                "current 1 Hz telemetry cannot attribute spikes to snapshot build/encode "
                "(no strong co-occurrence in sampled rows)"
            )
    else:
        note.append("no window-max ≥ 10 ms in this run")

    return {
        "spike_count_over_10ms": len(over),
        "top_spikes": top,
        "notes": "; ".join(note),
    }


def summarize_run(run_dir: Path) -> dict:
    summary = load_json(run_dir / "run_summary.json")
    config = load_json(run_dir / "config.json")
    rows = load_csv(run_dir / "metrics.csv")
    elapsed = f(summary.get("elapsed_secs"), f(config.get("duration_secs"), 1.0)) or 1.0
    bytes_out = f(summary.get("server_bytes_out"), 0.0) or 0.0
    bytes_in = f(summary.get("server_bytes_in"), 0.0) or 0.0
    peak = int(f(summary.get("peak_connected"), f(config.get("count"), 0)) or 0)
    requested = int(f(summary.get("requested_bots"), f(config.get("count"), 0)) or 0)

    out_rates = col(rows, "server_bytes_out_per_sec")
    in_rates = col(rows, "server_bytes_in_per_sec")
    snap_rates = col(rows, "server_snapshot_msgs_per_sec")
    cmd_rates = col(rows, "server_input_msgs_per_sec")
    mem_series = col(rows, "server_memory_mb")
    q_in = col(rows, "server_input_queue_current")
    q_sess = col(rows, "server_session_queue_max")

    out_mbps = (bytes_out / elapsed) / (1024 * 1024)
    mean_out_rate = statistics.mean(out_rates) if out_rates else (bytes_out / elapsed)
    mean_in_rate = statistics.mean(in_rates) if in_rates else (bytes_in / elapsed)

    # Queue trend: compare early vs late thirds of non-empty samples
    def trend(series: list[float]) -> str:
        if len(series) < 9:
            return "insufficient"
        n = len(series)
        early = statistics.mean(series[: n // 3])
        late = statistics.mean(series[-(n // 3) :])
        if late > early * 1.5 + 1.0:
            return "rising"
        if late + 0.5 < early * 0.7:
            return "falling"
        return "stable"

    row = {
        "run_dir": str(run_dir).replace("\\", "/"),
        "run_name": run_dir.name,
        "requested_bots": requested,
        "peak_connected": peak,
        "run_status": summary.get("run_status") or summary.get("status"),
        "status_reasons": summary.get("status_reasons") or [],
        "elapsed_secs": elapsed,
        "profile": config.get("profile"),
        "scenario": config.get("scenario"),
        "seed": config.get("seed"),
        "server_tick_work_mean_ms": summary.get("server_tick_work_mean_ms"),
        "server_tick_work_p95_ms": summary.get("server_tick_work_p95_ms"),
        "server_tick_work_p99_ms": summary.get("server_tick_work_p99_ms"),
        "server_tick_work_max_ms": summary.get("server_tick_work_max_ms"),
        "server_tick_overruns_total": summary.get("server_tick_overruns_total"),
        "server_scheduler_lateness_p95_ms": summary.get("server_scheduler_lateness_p95_ms"),
        "server_scheduler_lateness_max_ms": summary.get("server_scheduler_lateness_max_ms"),
        "server_input_queue_max": summary.get("server_input_queue_max"),
        "server_session_queue_max": summary.get("server_session_queue_max"),
        "input_queue_trend": trend(q_in),
        "session_queue_trend": trend(q_sess),
        "server_memory_start_mb": summary.get("server_memory_start_mb"),
        "server_memory_peak_mb": summary.get("server_memory_peak_mb"),
        "server_memory_end_mb": summary.get("server_memory_end_mb"),
        "harness_memory_peak_mb": summary.get("harness_memory_peak_mb"),
        "server_bytes_in": bytes_in,
        "server_bytes_out": bytes_out,
        "outbound_mib_per_sec_avg": out_mbps,
        "outbound_bytes_per_sec_mean": mean_out_rate,
        "inbound_bytes_per_sec_mean": mean_in_rate,
        "snapshot_msgs_per_sec_mean": statistics.mean(snap_rates) if snap_rates else None,
        "input_msgs_per_sec_mean": statistics.mean(cmd_rates) if cmd_rates else None,
        "per_player_outbound_bytes_per_sec": (mean_out_rate / peak) if peak else None,
        "per_player_memory_peak_mb": (
            (f(summary.get("server_memory_peak_mb"), 0.0) or 0.0) / peak if peak else None
        ),
        "unexpected_disconnects": summary.get("unexpected_disconnects"),
        "admission_refusals": summary.get("admission_refusals"),
        "encode_failures": summary.get("encode_failures"),
        "overflow_events": summary.get("overflow_events"),
        "snapshot_starvation_samples": summary.get("snapshot_starvation_samples"),
        "tick_percentile_semantics": summary.get(
            "tick_percentile_semantics", "peak_of_window_percentiles"
        ),
        "spike_analysis": analyze_spikes(rows),
        "memory_growth_mb": None,
    }
    start = f(summary.get("server_memory_start_mb"))
    end = f(summary.get("server_memory_end_mb"))
    if start is not None and end is not None:
        row["memory_growth_mb"] = end - start
    if mem_series and len(mem_series) >= 9:
        row["memory_series_trend"] = trend(mem_series)
    else:
        row["memory_series_trend"] = "insufficient"

    row["capacity_class"] = classify_capacity(row)
    return row


def pct_change(cur, prev):
    if cur is None or prev is None:
        return None
    if prev == 0:
        return None
    return (cur - prev) / abs(prev) * 100.0


def add_deltas(rows: list[dict]) -> None:
    for i, row in enumerate(rows):
        if i == 0:
            row["vs_prev"] = {}
            continue
        prev = rows[i - 1]
        row["vs_prev"] = {
            "tick_mean_pct": pct_change(
                f(row.get("server_tick_work_mean_ms")),
                f(prev.get("server_tick_work_mean_ms")),
            ),
            "tick_p99_pct": pct_change(
                f(row.get("server_tick_work_p99_ms")),
                f(prev.get("server_tick_work_p99_ms")),
            ),
            "tick_max_pct": pct_change(
                f(row.get("server_tick_work_max_ms")),
                f(prev.get("server_tick_work_max_ms")),
            ),
            "memory_peak_pct": pct_change(
                f(row.get("server_memory_peak_mb")),
                f(prev.get("server_memory_peak_mb")),
            ),
            "outbound_rate_pct": pct_change(
                f(row.get("outbound_bytes_per_sec_mean")),
                f(prev.get("outbound_bytes_per_sec_mean")),
            ),
            "population_pct": pct_change(row["peak_connected"], prev["peak_connected"]),
        }


def write_markdown(path: Path, payload: dict) -> None:
    rows = payload["runs"]
    lines = [
        "# PURGATORY Phase 5.7 — Localhost capacity characterization",
        "",
        f"Generated: {payload['generated_at']}",
        "",
        f"**Note:** {PERCENTILE_NOTE}",
        "",
        "This is **localhost** capacity behavior, not production capacity.",
        "",
        "## Comparison table",
        "",
        "| Bots | Peak | Tick mean | p95* | p99* | Max | Overruns | Queue max (in/sess) | Mem peak MB | Out MiB/s | Disconnects | Class | Status |",
        "| ---: | ---: | --------: | ---: | ---: | --: | -------: | ------------------: | ----------: | --------: | ----------: | ----- | ------ |",
    ]
    for r in rows:
        lines.append(
            "| {req} | {peak} | {mean} | {p95} | {p99} | {mx} | {ov} | {qin}/{qs} | {mem} | {out} | {dc} | {cls} | {st} |".format(
                req=r["requested_bots"],
                peak=r["peak_connected"],
                mean=_fmt(r.get("server_tick_work_mean_ms")),
                p95=_fmt(r.get("server_tick_work_p95_ms")),
                p99=_fmt(r.get("server_tick_work_p99_ms")),
                mx=_fmt(r.get("server_tick_work_max_ms")),
                ov=r.get("server_tick_overruns_total"),
                qin=r.get("server_input_queue_max"),
                qs=r.get("server_session_queue_max"),
                mem=_fmt(r.get("server_memory_peak_mb")),
                out=_fmt(r.get("outbound_mib_per_sec_avg")),
                dc=r.get("unexpected_disconnects"),
                cls=r.get("capacity_class"),
                st=r.get("run_status"),
            )
        )
    lines.extend(["", "\\* " + PERCENTILE_NOTE, "", "## % change vs previous population", ""])
    for r in rows:
        vp = r.get("vs_prev") or {}
        if not vp:
            continue
        lines.append(
            f"- **{r['requested_bots']} bots:** tick_mean { _fmt(vp.get('tick_mean_pct')) }%, "
            f"p99* { _fmt(vp.get('tick_p99_pct')) }%, max { _fmt(vp.get('tick_max_pct')) }%, "
            f"mem { _fmt(vp.get('memory_peak_pct')) }%, outbound { _fmt(vp.get('outbound_rate_pct')) }%, "
            f"pop { _fmt(vp.get('population_pct')) }%"
        )

    lines.extend(["", "## Per-player approximations (not assumed linear)", ""])
    for r in rows:
        lines.append(
            f"- **{r['peak_connected']} connected:** "
            f"~{_fmt(r.get('per_player_outbound_bytes_per_sec'))} B/s out/player, "
            f"~{_fmt(r.get('per_player_memory_peak_mb'))} MB peak/player"
        )

    knee = payload.get("knee", {})
    lines.extend(["", "## Localhost capacity knee", ""])
    lk = knee.get("localhost_knee")
    if lk:
        bots = lk.get("bots") or lk.get("bots_connected")
        lines.append(
            f"- Approximate primary knee near **{bots} connected** "
            f"(requested {lk.get('requested')}): {lk.get('reason')}"
        )
        lines.append(f"- Evidence run: `{lk.get('run')}`")
    else:
        lines.append("- No clear knee within tested populations (still below hard bounds or linear).")
    if knee.get("correctness_onset"):
        c = knee["correctness_onset"]
        lines.append(
            f"- Correctness onset: {c.get('bots')} connected — {c.get('reason')}"
        )
    if knee.get("simulation_pressure_onset"):
        s = knee["simulation_pressure_onset"]
        lines.append(
            f"- Simulation/network pressure onset: {s.get('bots')} connected — {s.get('reason')}"
        )
    if knee.get("admission_wall"):
        a = knee["admission_wall"]
        lines.append(
            f"- Admission wall: connected={a.get('bots_connected')} requested={a.get('requested')} — {a.get('reason')}"
        )
    lines.append(f"- {knee.get('disclaimer', '')}")

    lines.extend(["", "## Spike notes (23.9 ms-class window maxima)", ""])
    for r in rows:
        sp = r.get("spike_analysis") or {}
        lines.append(
            f"- **{r['requested_bots']} bots:** count>=10ms={sp.get('spike_count_over_10ms')}; "
            f"{sp.get('notes')}"
        )

    lines.extend(["", "## Graphs", ""])
    for g in payload.get("graphs", []):
        lines.append(f"- [`graphs/{g}`](graphs/{g})")

    lines.extend(
        [
            "",
            "## Source runs",
            "",
        ]
    )
    for r in rows:
        lines.append(f"- `{r['run_dir']}`")

    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def _fmt(v):
    if v is None:
        return "—"
    if isinstance(v, float):
        return f"{v:.3f}"
    return str(v)


def try_graphs(graphs_dir: Path, rows: list[dict]) -> list[str]:
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        return []

    graphs_dir.mkdir(parents=True, exist_ok=True)
    created = []
    xs = [r["peak_connected"] for r in rows]

    def save(name, title, ylabel, series):
        fig, ax = plt.subplots(figsize=(9, 4.5))
        any_data = False
        for label, ys in series.items():
            pts = [(x, y) for x, y in zip(xs, ys) if y is not None]
            if not pts:
                continue
            any_data = True
            ax.plot([p[0] for p in pts], [p[1] for p in pts], marker="o", label=label)
        if not any_data:
            plt.close(fig)
            return
        ax.set_title(title)
        ax.set_xlabel("peak connected bots")
        ax.set_ylabel(ylabel)
        ax.grid(True, alpha=0.3)
        ax.legend(loc="best")
        fig.tight_layout()
        fig.savefig(graphs_dir / name, dpi=120)
        plt.close(fig)
        created.append(name)

    save(
        "tick_mean_vs_bots.png",
        "Tick work mean vs population (localhost)",
        "ms",
        {"mean": [f(r.get("server_tick_work_mean_ms")) for r in rows]},
    )
    save(
        "tick_window_percentiles_vs_bots.png",
        "Peak-of-window p95/p99 vs population*",
        "ms",
        {
            "window_p95_peak": [f(r.get("server_tick_work_p95_ms")) for r in rows],
            "window_p99_peak": [f(r.get("server_tick_work_p99_ms")) for r in rows],
        },
    )
    save(
        "tick_max_vs_bots.png",
        "Peak window tick max vs population",
        "ms",
        {"max": [f(r.get("server_tick_work_max_ms")) for r in rows]},
    )
    save(
        "memory_peak_vs_bots.png",
        "Server memory peak vs population",
        "MB",
        {"peak_mb": [f(r.get("server_memory_peak_mb")) for r in rows]},
    )
    save(
        "outbound_vs_bots.png",
        "Mean outbound throughput vs population",
        "bytes/s",
        {"out": [f(r.get("outbound_bytes_per_sec_mean")) for r in rows]},
    )
    save(
        "queues_vs_bots.png",
        "Queue peaks vs population",
        "depth",
        {
            "input_max": [f(r.get("server_input_queue_max")) for r in rows],
            "session_max": [f(r.get("server_session_queue_max")) for r in rows],
        },
    )
    save(
        "per_player_outbound_vs_bots.png",
        "Per-player outbound bytes/s vs population",
        "B/s per player",
        {"per_player": [f(r.get("per_player_outbound_bytes_per_sec")) for r in rows]},
    )
    save(
        "tick_mean_per_player_vs_bots.png",
        "Tick mean / connected vs population",
        "ms per connected bot",
        {
            "mean/bot": [
                (
                    f(r.get("server_tick_work_mean_ms")) / r["peak_connected"]
                    if r["peak_connected"] and f(r.get("server_tick_work_mean_ms")) is not None
                    else None
                )
                for r in rows
            ]
        },
    )
    return created


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "run_dirs",
        nargs="+",
        help="one or more logs/load/<run> directories",
    )
    parser.add_argument(
        "--out",
        default=None,
        help="output capacity dir (default: logs/load/capacity/<timestamp>)",
    )
    parser.add_argument("--workspace", default=".")
    args = parser.parse_args()
    workspace = Path(args.workspace).resolve()

    run_dirs = []
    for raw in args.run_dirs:
        p = Path(raw)
        if not p.is_absolute():
            p = workspace / p
        if not p.is_dir():
            raise SystemExit(f"missing run dir: {p}")
        run_dirs.append(p)

    rows = [summarize_run(d) for d in run_dirs]
    rows.sort(key=lambda r: (r["requested_bots"], r["peak_connected"]))
    add_deltas(rows)
    knee = find_knee(rows)

    stamp = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
    out = Path(args.out) if args.out else workspace / "logs" / "load" / "capacity" / stamp
    if not out.is_absolute():
        out = workspace / out
    out.mkdir(parents=True, exist_ok=True)
    graphs = try_graphs(out / "graphs", rows)

    payload = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "tick_budget_ms": TICK_BUDGET_MS,
        "tick_percentile_semantics": "peak_of_window_percentiles",
        "percentile_note": PERCENTILE_NOTE,
        "scope": "localhost_capacity_characterization",
        "not_production_capacity": True,
        "runs": rows,
        "knee": knee,
        "graphs": graphs,
    }
    (out / "capacity_summary.json").write_text(
        json.dumps(payload, indent=2), encoding="utf-8"
    )
    write_markdown(out / "capacity_summary.md", payload)
    print(f"Wrote {out}")
    print(f"  capacity_summary.json")
    print(f"  capacity_summary.md")
    for g in graphs:
        print(f"  graphs/{g}")


if __name__ == "__main__":
    main()
