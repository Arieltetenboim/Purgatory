#!/usr/bin/env python3
"""Offline analysis for PURGATORY Phase 5.7 load-run artifacts.

Reads a completed run directory (config.json, metrics.csv, events.ndjson,
run_summary.json) and writes report/summary.md plus graphs/ charts when
matplotlib is available. Does not mutate raw input files. Does not run during
load.

Supports both the expanded Phase 5.7 CSV schema and older reduced schemas.
Backfills missing summary aggregates from metrics.csv when needed.
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path


def resolve_run_dir(arg: str | None, latest: bool, workspace: Path) -> Path:
    load_root = workspace / "logs" / "load"
    if latest or arg is None:
        pointer = load_root / "last_finished.txt"
        if not pointer.is_file():
            raise SystemExit(
                "no last_finished.txt — run a load test to completion first "
                "(or pass an explicit run directory)"
            )
        name = pointer.read_text(encoding="utf-8").strip()
        return load_root / name
    path = Path(arg)
    if not path.is_absolute():
        path = workspace / path
    return path


def load_json(path: Path) -> dict:
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def load_metrics(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    with path.open(newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def load_events(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    events = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return events


def fnum(row: dict, key: str) -> float | None:
    raw = row.get(key, "")
    if raw is None or raw == "":
        return None
    try:
        return float(raw)
    except ValueError:
        return None


def first_present(row: dict, keys: list[str]) -> float | None:
    for key in keys:
        v = fnum(row, key)
        if v is not None:
            return v
    return None


def growth_note(start, end, peak) -> str:
    """Flag unexplained growth. Not a statistical leak proof."""
    if start is None or end is None or peak is None:
        return "insufficient samples"
    try:
        start_f, end_f, peak_f = float(start), float(end), float(peak)
    except (TypeError, ValueError):
        return "insufficient samples"
    if start_f <= 0:
        return "no unexplained monotonic growth flagged"
    grew = end_f > start_f * 1.15
    near_peak = peak_f > 0 and (end_f >= start_f + 0.8 * (peak_f - start_f))
    if grew and near_peak:
        return "FLAG unexplained monotonic growth (investigate; not a leak proof)"
    return "no unexplained monotonic growth flagged"


def workload_line(config: dict, scenario: dict) -> str:
    bots = scenario.get("bot_count", config.get("count", "?"))
    synthetic = 0
    validation = scenario.get("validation") or {}
    if isinstance(validation, dict):
        synthetic = validation.get("synthetic_entities", 0)
    return f"- workload: bots={bots} synthetic_entities={synthetic}"
    """Per-metric aggregation matching harness ServerRunAggregator semantics."""
    tick_mean_sum = 0.0
    tick_mean_n = 0
    tick_p95_peak = None
    tick_p99_peak = None
    tick_max_peak = None
    last_overruns = None
    last_bytes_in = None
    last_bytes_out = None
    memory_start = None
    memory_peak = None
    memory_end = None
    input_q_max = None
    session_q_max = None
    sched_start = None
    sched_end = None
    sched_max = None
    actions_max = None
    spawn_max = None
    aoi_enters = None
    aoi_leaves = None
    aoi_updates = None
    ceiling_hits = None
    deferred_exh = None
    samples_ok = 0
    samples_missed = 0

    for row in metrics:
        mean = first_present(row, ["server_tick_work_mean_ms"])
        p95 = first_present(row, ["server_tick_work_p95_ms", "server_tick_p95_ms"])
        p99 = first_present(row, ["server_tick_work_p99_ms", "server_tick_p99_ms"])
        tmax = first_present(row, ["server_tick_work_max_ms", "server_tick_max_ms"])
        overruns = first_present(row, ["server_tick_overruns_total", "server_tick_overrun"])
        mem = first_present(row, ["server_memory_mb"])
        mem_peak = first_present(row, ["server_memory_peak_mb"])
        in_q = first_present(row, ["server_input_queue_max"])
        sess_q = first_present(row, ["server_session_queue_max"])
        # Cumulative counters are not always in old CSV; rates may exist only.
        # Prefer last non-empty session/overrun style fields when present.

        any_server = any(
            v is not None
            for v in (mean, p95, p99, tmax, overruns, mem, mem_peak, in_q, sess_q)
        ) or first_present(row, ["server_active_sessions"]) is not None

        if not any_server:
            samples_missed += 1
            continue
        samples_ok += 1
        if mean is not None:
            tick_mean_sum += mean
            tick_mean_n += 1
        if p95 is not None:
            tick_p95_peak = p95 if tick_p95_peak is None else max(tick_p95_peak, p95)
        if p99 is not None:
            tick_p99_peak = p99 if tick_p99_peak is None else max(tick_p99_peak, p99)
        if tmax is not None:
            tick_max_peak = tmax if tick_max_peak is None else max(tick_max_peak, tmax)
        if overruns is not None:
            last_overruns = int(overruns)
        if mem is not None:
            if memory_start is None:
                memory_start = mem
            memory_end = mem
            memory_peak = mem if memory_peak is None else max(memory_peak, mem)
        if mem_peak is not None:
            memory_peak = mem_peak if memory_peak is None else max(memory_peak, mem_peak)
        if in_q is not None:
            input_q_max = int(in_q) if input_q_max is None else max(input_q_max, int(in_q))
        if sess_q is not None:
            session_q_max = (
                int(sess_q) if session_q_max is None else max(session_q_max, int(sess_q))
            )
        queued = first_present(row, ["server_scheduler_queued"])
        if queued is not None:
            q = int(queued)
            if sched_start is None:
                sched_start = q
            sched_end = q
            sched_max = q if sched_max is None else max(sched_max, q)
        act = first_present(row, ["server_actions_active"])
        if act is not None:
            actions_max = int(act) if actions_max is None else max(actions_max, int(act))
        spawn = first_present(row, ["server_spawn_queue_depth"])
        if spawn is not None:
            spawn_max = int(spawn) if spawn_max is None else max(spawn_max, int(spawn))
        enters = first_present(row, ["server_aoi_enters"])
        if enters is not None:
            aoi_enters = int(enters)
        leaves = first_present(row, ["server_aoi_leaves"])
        if leaves is not None:
            aoi_leaves = int(leaves)
        updates = first_present(row, ["server_aoi_updates"])
        if updates is not None:
            aoi_updates = int(updates)
        hits = first_present(row, ["server_scheduler_critical_ceiling_hits"])
        if hits is not None:
            ceiling_hits = int(hits)
        exh = first_present(row, ["server_scheduler_deferred_exhausted"])
        if exh is not None:
            deferred_exh = int(exh)

    # Bytes: CSV has per-sec rates; reconstruct cumulative only if summary already
    # has them. Leave None here when only rates exist.
    out = {
        "server_tick_work_mean_ms": (
            tick_mean_sum / tick_mean_n if tick_mean_n else None
        ),
        "server_tick_work_p95_ms": tick_p95_peak,
        "server_tick_work_p99_ms": tick_p99_peak,
        "server_tick_work_max_ms": tick_max_peak,
        "server_tick_overruns_total": last_overruns,
        "server_bytes_in": last_bytes_in,
        "server_bytes_out": last_bytes_out,
        "server_memory_start_mb": memory_start,
        "server_memory_peak_mb": memory_peak,
        "server_memory_end_mb": memory_end,
        "server_input_queue_max": input_q_max,
        "server_session_queue_max": session_q_max,
        "scheduler_queued_start": sched_start,
        "scheduler_queued_end": sched_end,
        "scheduler_queued_max": sched_max,
        "actions_active_max": actions_max,
        "spawn_queue_depth_max": spawn_max,
        "aoi_enters_total": aoi_enters,
        "aoi_leaves_total": aoi_leaves,
        "aoi_updates_total": aoi_updates,
        "scheduler_critical_ceiling_hits": ceiling_hits,
        "scheduler_deferred_exhausted": deferred_exh,
        "server_metrics_samples_ok": samples_ok,
        "server_metrics_samples_missed": samples_missed,
        "server_metrics_ok": samples_ok > 0,
    }
    return out


def enrich_summary(summary: dict, metrics: list[dict]) -> dict:
    """Fill missing aggregate keys from CSV without overwriting present values."""
    if not metrics:
        return summary
    agg = aggregate_from_csv(metrics)
    out = dict(summary)
    for key, value in agg.items():
        if value is None:
            continue
        cur = out.get(key)
        if cur is None or cur == "":
            out[key] = value
    # Normalize legacy status key.
    if "run_status" not in out and "status" in out:
        out["run_status"] = str(out["status"]).upper()
    if "status_reasons" not in out:
        out["status_reasons"] = out.get("status_reasons") or []
    return out


def status_block(summary: dict) -> list[str]:
    status = (
        summary.get("run_status")
        or summary.get("status")
        or "UNKNOWN"
    )
    reasons = summary.get("status_reasons") or []
    lines = [f"Status: {status}", ""]
    if reasons:
        lines.append("Reasons:")
        for r in reasons:
            if isinstance(r, dict):
                msg = r.get("message") or r.get("code") or str(r)
            else:
                msg = str(r)
            lines.append(f"- {msg}")
    elif str(status).upper() in ("COMPLETE",):
        lines.append("No correctness warnings detected.")
    elif str(status).upper() in ("WARN", "WARNING", "FAILED", "FAIL"):
        lines.append(
            "- (no status_reasons in summary — incomplete harness artifact; "
            "re-run with updated load harness)"
        )
    else:
        lines.append("(no status_reasons in summary — re-run with updated harness)")
    lines.append("")
    return lines


def write_summary(
    report_dir: Path,
    config: dict,
    summary: dict,
    metrics: list[dict],
    events: list[dict],
    graph_names: list[str],
    scenario: dict | None = None,
) -> Path:
    report_dir.mkdir(parents=True, exist_ok=True)
    scenario = scenario or {}
    mem_note = growth_note(
        summary.get("server_memory_start_mb"),
        summary.get("server_memory_end_mb"),
        summary.get("server_memory_peak_mb"),
    )
    sched_note = growth_note(
        summary.get("scheduler_queued_start"),
        summary.get("scheduler_queued_end"),
        summary.get("scheduler_queued_max"),
    )
    lines = [
        "# Load run report",
        "",
        *status_block(summary),
        "## Highlights",
        "",
        workload_line(config, scenario),
        f"- requested bots: {summary.get('requested_bots', config.get('count', '?'))}",
        f"- peak connected: {summary.get('peak_connected', '?')}",
        f"- duration (s): {summary.get('elapsed_secs', '?')}",
        f"- server metrics health: {summary.get('server_metrics_health', '?')} "
        f"(ok={summary.get('server_metrics_ok')}, "
        f"samples_ok={summary.get('server_metrics_samples_ok')}, "
        f"missed={summary.get('server_metrics_samples_missed')})",
        f"- server tick mean/p95/p99/max (ms): "
        f"{summary.get('server_tick_work_mean_ms')} / "
        f"{summary.get('server_tick_work_p95_ms')} / "
        f"{summary.get('server_tick_work_p99_ms')} / "
        f"{summary.get('server_tick_work_max_ms')}",
        f"- tick overruns: {summary.get('server_tick_overruns_total')}",
        f"- bytes in/out: {summary.get('server_bytes_in')} / {summary.get('server_bytes_out')}",
        f"- memory start/peak/end (MB): "
        f"{summary.get('server_memory_start_mb')} / "
        f"{summary.get('server_memory_peak_mb')} / "
        f"{summary.get('server_memory_end_mb')}",
        f"- memory trend: {mem_note}",
        f"- scheduler queued trend: {sched_note}",
        f"- harness memory peak (MB): {summary.get('harness_memory_peak_mb')}",
        f"- queue peak (input/session): "
        f"{summary.get('server_input_queue_max')} / "
        f"{summary.get('server_session_queue_max')}",
        f"- failure class: {summary.get('failure_class', '?')}",
        f"- scheduler queued start/end/max: "
        f"{summary.get('scheduler_queued_start')} / "
        f"{summary.get('scheduler_queued_end')} / "
        f"{summary.get('scheduler_queued_max')}",
        f"- actions_active max: {summary.get('actions_active_max')}",
        f"- spawn queue max: {summary.get('spawn_queue_depth_max')}",
        f"- AOI enter/leave/update totals: "
        f"{summary.get('aoi_enters_total')} / "
        f"{summary.get('aoi_leaves_total')} / "
        f"{summary.get('aoi_updates_total')}",
        f"- scheduler critical ceiling hits: "
        f"{summary.get('scheduler_critical_ceiling_hits')}",
        f"- scheduler deferred exhausted: "
        f"{summary.get('scheduler_deferred_exhausted')}",
        f"- snapshot encode failures: {summary.get('encode_failures')}",
        f"- unexpected disconnects: {summary.get('unexpected_disconnects')}",
        f"- snapshot starvation samples: {summary.get('snapshot_starvation_samples')}",
        "",
        "## Graphs",
        "",
    ]
    if graph_names:
        for name in graph_names:
            lines.append(f"- [`graphs/{name}`](../graphs/{name})")
    else:
        lines.append("- (none generated)")
    lines.extend(
        [
            "",
            "## Configuration",
            "",
            "```json",
            json.dumps(config, indent=2),
            "```",
            "",
            "## Summary JSON",
            "",
            "```json",
            json.dumps(summary, indent=2),
            "```",
            "",
            f"- metric samples: {len(metrics)}",
            f"- events: {len(events)}",
            "",
            "## Notes",
            "",
            "- Empty server metric cells mean a missed UDP poll (not zero).",
            "- Aggregates are computed per-metric from valid samples only.",
            "- `bot_scheduler_*` is harness wall-clock cadence, not server simulation cost.",
            "- `server_tick_work_*` summary: mean = avg of sample means; "
            "p95/p99/max = peak across samples.",
            "- Schema 2/3 AOI and runtime gauges are recorded when present; "
            "empty cells are missed polls, not zeros.",
            "- Soak trends report start/end/max. Unexplained monotonic growth "
            "is a finding, not a statistical leak proof.",
            "- Snapshot sequence gaps are application coalescing, not packet loss.",
            "- Localhost shared-CPU numbers are not production capacity.",
            "",
        ]
    )
    out = report_dir / "summary.md"
    out.write_text("\n".join(lines), encoding="utf-8")
    return out


def _downsample(
    xs: list[float], series_map: dict[str, list[float | None]], max_points: int = 1800
) -> tuple[list[float], dict[str, list[float | None]]]:
    n = len(xs)
    if n <= max_points:
        return xs, series_map
    stride = max(1, n // max_points)
    idx = list(range(0, n, stride))
    if idx[-1] != n - 1:
        idx.append(n - 1)
    xs2 = [xs[i] for i in idx]
    out = {k: [v[i] for i in idx] for k, v in series_map.items()}
    return xs2, out


def try_charts(
    graphs_dir: Path, metrics: list[dict]
) -> tuple[list[str], str | None]:
    """Returns (created_chart_names, skip_reason). Never raises."""
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        return [], "matplotlib is not installed"

    created: list[str] = []
    if not metrics:
        return created, "no metrics rows"

    try:
        graphs_dir.mkdir(parents=True, exist_ok=True)
        xs = [first_present(r, ["elapsed_secs"]) or 0.0 for r in metrics]

        def series(*keys: str) -> list[float | None]:
            return [first_present(r, list(keys)) for r in metrics]

        def plot_xy(
            name: str,
            title: str,
            ys_map: dict[str, list[float | None]],
            ref_y: float | None = None,
            ylabel: str | None = None,
            xlabel: str = "elapsed time (s)",
        ):
            xs_p, ys_p = _downsample(xs, ys_map)
            fig, ax = plt.subplots(figsize=(11, 4.2))
            any_data = False
            for label, ys in ys_p.items():
                pts = [(x, y) for x, y in zip(xs_p, ys) if y is not None]
                if not pts:
                    continue
                any_data = True
                ax.plot([p[0] for p in pts], [p[1] for p in pts], label=label, linewidth=1.2)
            if ref_y is not None:
                ax.axhline(
                    ref_y, color="gray", linestyle="--", linewidth=1, label="33.333 ms budget"
                )
            if not any_data:
                plt.close(fig)
                return
            ax.set_title(title)
            ax.set_xlabel(xlabel)
            if ylabel:
                ax.set_ylabel(ylabel)
            ax.legend(loc="best")
            ax.grid(True, alpha=0.3)
            path = graphs_dir / name
            fig.tight_layout()
            fig.savefig(path, dpi=120)
            plt.close(fig)
            created.append(name)

        plot_xy(
            "population.png",
            "Connected bots / server sessions over time",
            {
                "connected": series("connected_bots", "connected"),
                "target": series("target_bots"),
                "server_sessions": series("server_active_sessions"),
            },
            ylabel="count",
        )
        plot_xy(
            "tick_timing.png",
            "Server tick work over time",
            {
                "server_p50": series("server_tick_work_p50_ms", "server_tick_p50_ms"),
                "server_p95": series("server_tick_work_p95_ms", "server_tick_p95_ms"),
                "server_p99": series("server_tick_work_p99_ms", "server_tick_p99_ms"),
                "server_max": series("server_tick_work_max_ms", "server_tick_max_ms"),
                "bot_sched_p99": series("bot_scheduler_p99_ms", "tick_p99_ms"),
            },
            ref_y=33.333,
            ylabel="milliseconds",
        )
        plot_xy(
            "bandwidth.png",
            "Network throughput over time",
            {
                "bytes_in/s": series("server_bytes_in_per_sec"),
                "bytes_out/s": series("server_bytes_out_per_sec"),
            },
            ylabel="bytes / s",
        )
        plot_xy(
            "memory.png",
            "Memory working set over time",
            {
                "server_mb": series("server_memory_mb"),
                "server_peak_mb": series("server_memory_peak_mb"),
                "harness_mb": series("harness_memory_mb"),
            },
            ylabel="megabytes",
        )
        plot_xy(
            "queues.png",
            "Input / session queue depth over time",
            {
                "input_current": series("server_input_queue_current"),
                "input_max": series("server_input_queue_max"),
                "session_max": series("server_session_queue_max"),
            },
            ylabel="queue depth",
        )
        plot_xy(
            "snapshot_cost.png",
            "Snapshot build / encode cost over time",
            {
                "build_max_ms": series("snapshot_build_time_max_ms"),
                "encode_max_ms": series("snapshot_encode_time_max_ms"),
                "size_max_bytes": series("snapshot_size_max_bytes"),
                "build_count": series("snapshot_build_count"),
            },
            ylabel="mixed units",
        )
        plot_xy(
            "runtime.png",
            "Scheduler / action / spawn gauges over time",
            {
                "scheduler_queued": series("server_scheduler_queued"),
                "actions_active": series("server_actions_active"),
                "spawn_queue": series("server_spawn_queue_depth"),
            },
            ylabel="count",
        )
        plot_xy(
            "aoi.png",
            "AOI Enter / Update / Leave (cumulative)",
            {
                "enters": series("server_aoi_enters"),
                "updates": series("server_aoi_updates"),
                "leaves": series("server_aoi_leaves"),
            },
            ylabel="records",
        )
        return created, None
    except Exception as exc:  # noqa: BLE001 — chart failures must not abort the report
        return created, f"chart rendering failed: {exc}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_dir", nargs="?", help="path to a logs/load/<run> directory")
    parser.add_argument(
        "--latest",
        action="store_true",
        help="use logs/load/last_finished.txt",
    )
    parser.add_argument(
        "--workspace",
        default=".",
        help="workspace root (default: cwd)",
    )
    args = parser.parse_args()
    workspace = Path(args.workspace).resolve()
    run_dir = resolve_run_dir(args.run_dir, args.latest, workspace)
    if not run_dir.is_dir():
        raise SystemExit(f"run directory not found: {run_dir}")

    print(f"Analyzing:\n{run_dir}")

    config = load_json(run_dir / "config.json")
    scenario = load_json(run_dir / "scenario.json")
    summary_raw = load_json(run_dir / "run_summary.json")
    metrics = load_metrics(run_dir / "metrics.csv")
    events = load_events(run_dir / "events.ndjson")

    if not summary_raw:
        print(
            f"warning: {run_dir / 'run_summary.json'} missing — "
            "run may still be in progress or was aborted without a summary",
            file=sys.stderr,
        )

    summary = enrich_summary(summary_raw, metrics)

    graphs_dir = run_dir / "graphs"
    charts, skip_reason = try_charts(graphs_dir, metrics)

    report_dir = run_dir / "report"
    summary_path = write_summary(
        report_dir, config, summary, metrics, events, charts, scenario
    )

    print(f"\nReport generated:\n  {summary_path.relative_to(run_dir)}")
    if charts:
        for name in charts:
            print(f"  graphs/{name}")
    elif skip_reason:
        print("\nReport generated with warnings:")
        if skip_reason == "matplotlib is not installed":
            print("  charts skipped: matplotlib is not installed.")
        else:
            print(f"  charts skipped: {skip_reason}")
    else:
        print("  (no plottable columns for charts)")


if __name__ == "__main__":
    main()
