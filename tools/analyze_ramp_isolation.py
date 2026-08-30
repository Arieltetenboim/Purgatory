#!/usr/bin/env python3
"""Phase 5.7 ramp vs steady-state isolation report.

Reads completed load-run directories from a 3x3 matrix (population x ramp_ms)
and writes logs/load/capacity/<stamp>/ramp_isolation/{ramp_summary.json,md,graphs/}.

Primary X-axis for performance comparisons: peak connected bots (not requested).
"""

from __future__ import annotations

import argparse
import csv
import json
import statistics
import sys
from datetime import datetime, timezone
from pathlib import Path


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


def load_events(path: Path) -> list[dict]:
    if not path.is_file():
        return []
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def col(rows: list[dict], *keys: str) -> list[tuple[float, float]]:
    """Return (elapsed, value) pairs for first present key."""
    pts = []
    for r in rows:
        elapsed = f(r.get("elapsed_secs"))
        if elapsed is None:
            continue
        for k in keys:
            v = f(r.get(k))
            if v is not None:
                pts.append((elapsed, v))
                break
    return pts


def queue_recovery(rows: list[dict], ramp_ts: float | None) -> dict:
    """Compare queue depth during ramp vs last third of run."""
    if ramp_ts is None or not rows:
        return {"input": "unknown", "session": "unknown"}

    def series(key_cur: str, key_max: str):
        during = []
        after = []
        late = []
        total_elapsed = f(rows[-1].get("elapsed_secs"), 0.0) or 0.0
        late_start = total_elapsed * 2.0 / 3.0
        for r in rows:
            t = f(r.get("elapsed_secs"), 0.0) or 0.0
            v = f(r.get(key_cur))
            if v is None:
                v = f(r.get(key_max))
            if v is None:
                continue
            if t <= ramp_ts:
                during.append(v)
            else:
                after.append(v)
            if t >= late_start:
                late.append(v)
        return during, after, late

    def label(during, after, late):
        if not during and not after:
            return "insufficient"
        d = max(during) if during else 0.0
        a_mean = statistics.mean(after) if after else None
        l_mean = statistics.mean(late) if late else a_mean
        if l_mean is None:
            return "insufficient"
        # Recovered if late mean near empty and not growing vs post-ramp
        if l_mean <= 1.5 and (d == 0 or l_mean <= max(1.0, d * 0.5)):
            return "recovered"
        if after and late and l_mean > statistics.mean(after) * 1.25 + 0.5:
            return "accumulating"
        if l_mean > 2.0:
            return "elevated_stable"
        return "recovered"

    idur, iaft, ilate = series("server_input_queue_current", "server_input_queue_max")
    sdur, saft, slate = series("server_session_queue_max", "server_session_queue_max")
    return {
        "input": label(idur, iaft, ilate),
        "session": label(sdur, saft, slate),
        "input_peak_during_ramp": max(idur) if idur else None,
        "input_mean_late": statistics.mean(ilate) if ilate else None,
        "session_peak_during_ramp": max(sdur) if sdur else None,
        "session_mean_late": statistics.mean(slate) if slate else None,
    }


def drop_phase(first_drop_ts: float | None, ramp_ts: float | None, has_steady_event: bool) -> str:
    if first_drop_ts is None:
        return "none"
    if ramp_ts is None:
        return "unknown"
    if first_drop_ts <= ramp_ts:
        phase = "during_ramp"
    elif first_drop_ts <= ramp_ts + 15.0:
        phase = "shortly_after_ramp"
    else:
        phase = "steady_state"
    if has_steady_event and phase != "steady_state":
        return phase + "+steady_continuation"
    if has_steady_event:
        return "steady_state"
    return phase


def analyze_run(run_dir: Path) -> dict:
    summary = load_json(run_dir / "run_summary.json")
    config = load_json(run_dir / "config.json")
    rows = load_csv(run_dir / "metrics.csv")
    events = load_events(run_dir / "events.ndjson")

    requested = int(f(summary.get("requested_bots"), f(config.get("count"), 0)) or 0)
    peak = int(f(summary.get("peak_connected"), 0) or 0)
    ramp_ms = int(f(config.get("ramp_ms"), 0) or 0)
    elapsed = f(summary.get("elapsed_secs"), f(config.get("duration_secs"), 1.0)) or 1.0

    ramp_ts = None
    first_drop_ts = None
    first_drop_handoff = None
    first_drop_ramp_complete = None
    has_steady_drop = False
    spike_events = []
    for ev in events:
        kind = ev.get("event")
        ts = f(ev.get("ts"))
        if kind == "ramp_target_reached" and ts is not None:
            ramp_ts = ts
        if kind == "queue_overflow" and first_drop_ts is None and ts is not None:
            first_drop_ts = ts
            first_drop_handoff = ev.get("input_handoff_dropped")
            first_drop_ramp_complete = ev.get("ramp_complete")
        if kind == "input_handoff_drop_steady":
            has_steady_drop = True
        if kind == "tick_work_spike":
            spike_events.append(ev)

    handoff_total = f(summary.get("input_handoff_dropped_total"))
    if handoff_total is None:
        # Fall back to first overflow event or summary overflow_events.
        handoff_total = f(first_drop_handoff, f(summary.get("overflow_events"), 0) or 0)

    out_rates = [v for _, v in col(rows, "server_bytes_out_per_sec")]
    in_rates = [v for _, v in col(rows, "server_bytes_in_per_sec")]
    snap_rates = [v for _, v in col(rows, "server_snapshot_msgs_per_sec")]
    cmd_rates = [v for _, v in col(rows, "server_input_msgs_per_sec")]
    bytes_out = f(summary.get("server_bytes_out"), 0.0) or 0.0
    bytes_in = f(summary.get("server_bytes_in"), 0.0) or 0.0
    mean_out = statistics.mean(out_rates) if out_rates else bytes_out / elapsed
    mean_in = statistics.mean(in_rates) if in_rates else bytes_in / elapsed

    # Spike correlation from events
    spike_with_snap = 0
    for sp in spike_events:
        b = f(sp.get("snapshot_build_time_max_ms"), 0.0) or 0.0
        e = f(sp.get("snapshot_encode_time_max_ms"), 0.0) or 0.0
        if b >= 1.0 or e >= 1.0:
            spike_with_snap += 1

    recovery = queue_recovery(rows, ramp_ts)
    phase = drop_phase(first_drop_ts, ramp_ts, has_steady_drop)

    return {
        "run_dir": str(run_dir).replace("\\", "/"),
        "run_name": run_dir.name,
        "requested_bots": requested,
        "peak_connected": peak,
        "ramp_ms": ramp_ms,
        "seed": config.get("seed"),
        "profile": config.get("profile"),
        "scenario": config.get("scenario"),
        "duration_secs": elapsed,
        "run_status": summary.get("run_status") or summary.get("status"),
        "status_reasons": summary.get("status_reasons") or [],
        "input_handoff_dropped_total": int(handoff_total or 0),
        "first_drop_secs": first_drop_ts,
        "first_drop_ramp_complete_flag": first_drop_ramp_complete,
        "drop_phase": phase,
        "steady_drop_event": has_steady_drop,
        "ramp_complete_secs": ramp_ts,
        "unexpected_disconnects": summary.get("unexpected_disconnects"),
        "snapshot_starvation_samples": summary.get("snapshot_starvation_samples"),
        "overflow_events": summary.get("overflow_events"),
        "encode_failures": summary.get("encode_failures"),
        "admission_refusals": summary.get("admission_refusals"),
        "server_tick_work_mean_ms": summary.get("server_tick_work_mean_ms"),
        "server_tick_work_p95_ms": summary.get("server_tick_work_p95_ms"),
        "server_tick_work_p99_ms": summary.get("server_tick_work_p99_ms"),
        "server_tick_work_max_ms": summary.get("server_tick_work_max_ms"),
        "server_tick_overruns_total": summary.get("server_tick_overruns_total"),
        "tick_percentile_semantics": summary.get(
            "tick_percentile_semantics", "peak_of_window_percentiles"
        ),
        "server_input_queue_max": summary.get("server_input_queue_max"),
        "server_session_queue_max": summary.get("server_session_queue_max"),
        "queue_recovery": recovery,
        "outbound_mib_per_sec": (mean_out / (1024 * 1024)),
        "inbound_mib_per_sec": (mean_in / (1024 * 1024)),
        "outbound_bytes_per_sec_mean": mean_out,
        "inbound_bytes_per_sec_mean": mean_in,
        "snapshot_msgs_per_sec_mean": statistics.mean(snap_rates) if snap_rates else None,
        "input_msgs_per_sec_mean": statistics.mean(cmd_rates) if cmd_rates else None,
        "per_connected_outbound_bytes_per_sec": (mean_out / peak) if peak else None,
        "server_memory_peak_mb": summary.get("server_memory_peak_mb"),
        "server_memory_end_mb": summary.get("server_memory_end_mb"),
        "harness_memory_peak_mb": summary.get("harness_memory_peak_mb"),
        "tick_spike_events": len(spike_events),
        "tick_spikes_with_elevated_snapshot": spike_with_snap,
        "metrics_rows": len(rows),
        "time_series": {
            "elapsed": [f(r.get("elapsed_secs"), 0.0) for r in rows],
            "connected": [
                f(r.get("connected_bots") or r.get("connected"), 0.0) for r in rows
            ],
            "input_q": [f(r.get("server_input_queue_current")) for r in rows],
            "session_q_max": [f(r.get("server_session_queue_max")) for r in rows],
            "tick_max": [
                f(r.get("server_tick_work_max_ms") or r.get("server_tick_max_ms"))
                for r in rows
            ],
        },
    }


def classify_limitation(runs: list[dict]) -> dict:
    """RAMP-LIMITED / STEADY-STATE INPUT-LIMITED / MIXED / INCONCLUSIVE."""
    by_key = {(r["peak_connected"], r["ramp_ms"]): r for r in runs}
    # Focus on populations that fully connected.
    pops = sorted({r["peak_connected"] for r in runs if r["peak_connected"] > 0})

    def drops(pop, ramp):
        r = by_key.get((pop, ramp))
        return int(r["input_handoff_dropped_total"]) if r else None

    answers = {}
    for pop in (100, 200, 256):
        answers[pop] = {
            50: drops(pop, 50),
            200: drops(pop, 200),
            500: drops(pop, 500),
        }

    # A/B/C: does slowing eliminate drops?
    def eliminated(pop):
        d50 = answers.get(pop, {}).get(50)
        d200 = answers.get(pop, {}).get(200)
        d500 = answers.get(pop, {}).get(500)
        if None in (d50, d200, d500):
            return None
        slow_ok = (d200 == 0 and d500 == 0)
        fast_bad = d50 > 0
        return {"fast_had_drops": fast_bad, "slow_eliminated": slow_ok and fast_bad, "all_zero": d50 == d200 == d500 == 0}

    elim = {p: eliminated(p) for p in (100, 200, 256)}

    phases = {r["drop_phase"] for r in runs if r["input_handoff_dropped_total"] > 0}
    steady_phases = any(
        "steady" in (r["drop_phase"] or "") for r in runs if r["input_handoff_dropped_total"] > 0
    )
    only_ramp = all(
        (r["drop_phase"] or "").startswith("during_ramp")
        or (r["drop_phase"] or "") == "none"
        for r in runs
    ) and any(r["input_handoff_dropped_total"] > 0 for r in runs)

    recoveries = [r["queue_recovery"]["input"] for r in runs]
    sess_rec = [r["queue_recovery"]["session"] for r in runs]

    # Steady-state sim budget: mean << 33ms and max < 33 for slow-ramp fully-connected runs
    slow_ok_sim = []
    for r in runs:
        if r["ramp_ms"] >= 200 and r["peak_connected"] == r["requested_bots"]:
            mean = f(r.get("server_tick_work_mean_ms"), 0.0) or 0.0
            mx = f(r.get("server_tick_work_max_ms"), 0.0) or 0.0
            slow_ok_sim.append(mean < 5.0 and mx < 33.333 and int(r.get("server_tick_overruns_total") or 0) == 0)

    # Classification
    # Prefer MIXED when onset is during ramp AND drops continue / survive slow ramps.
    if steady_phases and any(
        (r.get("drop_phase") or "").startswith("during_ramp")
        for r in runs
        if r["input_handoff_dropped_total"] > 0
    ) and any(
        elim[p] and not elim[p]["slow_eliminated"] and not elim[p]["all_zero"]
        for p in (100, 200, 256)
        if elim[p] is not None
    ):
        clazz = "MIXED"
        note = (
            "Drops often begin during ramp and continue after ramp completion; "
            "slowing the ramp does not eliminate them at 200/256."
        )
    elif all(elim[p] and elim[p]["all_zero"] for p in (100, 200, 256) if elim[p] is not None):
        clazz = "INCONCLUSIVE"  # no drops anywhere — prior failure not reproduced
        note = "No input_handoff_dropped in this matrix; cannot confirm prior failure mode."
    elif all(
        elim[p] and (elim[p]["slow_eliminated"] or elim[p]["all_zero"])
        for p in (100, 200, 256)
        if elim[p] is not None
    ) and only_ramp:
        clazz = "RAMP-LIMITED"
        note = "Drops appear under fast ramp and are eliminated or confined to ramp with slower ramps."
    elif steady_phases and any(
        elim[p] and not elim[p]["slow_eliminated"] and not elim[p]["all_zero"]
        for p in (100, 200, 256)
        if elim[p] is not None
    ):
        clazz = "STEADY-STATE INPUT-LIMITED"
        note = "Drops persist with slower ramps and/or continue after ramp completion."
    elif any(r["input_handoff_dropped_total"] > 0 for r in runs) and (
        any(elim[p] and elim[p]["slow_eliminated"] for p in (100, 200, 256) if elim[p])
        and steady_phases
    ):
        clazz = "MIXED"
        note = "Fast ramp causes drops; some steady-state continuation or residual drops remain."
    elif any(r["input_handoff_dropped_total"] > 0 for r in runs):
        # Heuristic mix/ramp from phases
        if only_ramp and any(elim[p] and elim[p].get("slow_eliminated") for p in elim if elim[p]):
            clazz = "RAMP-LIMITED"
            note = "Drops tied to connection ramp timing."
        elif steady_phases:
            clazz = "MIXED"
            note = "Evidence of both ramp and post-ramp drop activity."
        else:
            clazz = "INCONCLUSIVE"
            note = "Drops observed but phase/ramp relationship is ambiguous."
    else:
        clazz = "INCONCLUSIVE"
        note = "Insufficient drop signal."

    return {
        "classification": clazz,
        "note": note,
        "elimination_by_population": elim,
        "drop_phases_seen": sorted(phases),
        "queue_input_recovery": recoveries,
        "queue_session_recovery": sess_rec,
        "slow_ramp_sim_within_budget": all(slow_ok_sim) if slow_ok_sim else None,
        "answers": {
            "A_100_slow_eliminates": (elim.get(100) or {}).get("slow_eliminated"),
            "A_100_all_zero": (elim.get(100) or {}).get("all_zero"),
            "B_200_slow_eliminates": (elim.get(200) or {}).get("slow_eliminated"),
            "B_200_all_zero": (elim.get(200) or {}).get("all_zero"),
            "C_256_slow_eliminates": (elim.get(256) or {}).get("slow_eliminated"),
            "C_256_all_zero": (elim.get(256) or {}).get("all_zero"),
            "D_drop_phases": sorted(phases),
            "E_queues_recover": {
                "input": recoveries,
                "session": sess_rec,
            },
            "F_steady_sim_budget_ok": all(slow_ok_sim) if slow_ok_sim else None,
            "G_spikes_correlate_snapshot": any(
                r["tick_spikes_with_elevated_snapshot"] > 0 for r in runs
            ),
        },
    }


def try_graphs(out_dir: Path, runs: list[dict]) -> list[str]:
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        return []

    gdir = out_dir / "graphs"
    gdir.mkdir(parents=True, exist_ok=True)
    created = []
    ramps = sorted({r["ramp_ms"] for r in runs})
    colors = {50: "C0", 200: "C1", 500: "C2"}

    def grouped(ykey, title, ylabel, fname):
        fig, ax = plt.subplots(figsize=(9, 4.5))
        for ramp in ramps:
            subset = sorted(
                [r for r in runs if r["ramp_ms"] == ramp],
                key=lambda r: r["peak_connected"],
            )
            xs = [r["peak_connected"] for r in subset]
            ys = [f(r.get(ykey)) for r in subset]
            pts = [(x, y) for x, y in zip(xs, ys) if y is not None]
            if not pts:
                continue
            ax.plot(
                [p[0] for p in pts],
                [p[1] for p in pts],
                marker="o",
                color=colors.get(ramp, None),
                label=f"ramp {ramp} ms",
            )
        ax.set_title(title)
        ax.set_xlabel("peak connected bots")
        ax.set_ylabel(ylabel)
        ax.grid(True, alpha=0.3)
        ax.legend(loc="best")
        fig.tight_layout()
        fig.savefig(gdir / fname, dpi=120)
        plt.close(fig)
        created.append(fname)

    grouped(
        "input_handoff_dropped_total",
        "input_handoff_dropped vs peak connected (by ramp)",
        "drops (total)",
        "handoff_drops_vs_connected.png",
    )
    grouped(
        "server_input_queue_max",
        "Input queue peak vs peak connected (by ramp)",
        "queue depth",
        "input_queue_vs_connected.png",
    )
    grouped(
        "server_session_queue_max",
        "Session queue peak vs peak connected (by ramp)",
        "queue depth",
        "session_queue_vs_connected.png",
    )
    grouped(
        "server_tick_work_mean_ms",
        "Tick work mean vs peak connected (by ramp)",
        "ms",
        "tick_mean_vs_connected.png",
    )
    grouped(
        "outbound_mib_per_sec",
        "Outbound MiB/s vs peak connected (by ramp)",
        "MiB/s",
        "outbound_vs_connected.png",
    )

    # Time-series: pick worst fast-ramp fully connected run if present else first with drops
    candidates = [
        r
        for r in runs
        if r["ramp_ms"] == 50 and r["peak_connected"] == r["requested_bots"]
    ]
    if not candidates:
        candidates = runs
    focus = max(candidates, key=lambda r: r["input_handoff_dropped_total"])
    ts = focus["time_series"]
    fig, ax1 = plt.subplots(figsize=(11, 4.5))
    ax1.plot(ts["elapsed"], ts["connected"], label="connected", color="C0")
    ax1.set_xlabel("elapsed (s)")
    ax1.set_ylabel("connected bots", color="C0")
    ax2 = ax1.twinx()
    iq = [v if v is not None else float("nan") for v in ts["input_q"]]
    sq = [v if v is not None else float("nan") for v in ts["session_q_max"]]
    ax2.plot(ts["elapsed"], iq, label="input_q", color="C1", alpha=0.8)
    ax2.plot(ts["elapsed"], sq, label="session_q_max", color="C2", alpha=0.8)
    if focus.get("ramp_complete_secs") is not None:
        ax1.axvline(
            focus["ramp_complete_secs"],
            color="gray",
            linestyle="--",
            label="ramp complete",
        )
    if focus.get("first_drop_secs") is not None:
        ax1.axvline(
            focus["first_drop_secs"],
            color="red",
            linestyle=":",
            label="first handoff drop",
        )
    ax2.set_ylabel("queue depth")
    ax1.set_title(
        f"Time series: connected vs queues "
        f"(req={focus['requested_bots']} ramp={focus['ramp_ms']}ms peak={focus['peak_connected']})"
    )
    lines1, lab1 = ax1.get_legend_handles_labels()
    lines2, lab2 = ax2.get_legend_handles_labels()
    ax1.legend(lines1 + lines2, lab1 + lab2, loc="best")
    fig.tight_layout()
    fig.savefig(gdir / "timeseries_ramp_queues.png", dpi=120)
    plt.close(fig)
    created.append("timeseries_ramp_queues.png")
    return created


def _fmt(v):
    if v is None:
        return "—"
    if isinstance(v, float):
        return f"{v:.3f}"
    return str(v)


def write_md(path: Path, payload: dict) -> None:
    runs = payload["runs"]
    lines = [
        "# PURGATORY Phase 5.7 — Ramp vs steady-state isolation",
        "",
        f"Generated: {payload['generated_at']}",
        "",
        "Primary performance X-axis: **peak connected bots** (not requested).",
        "Admission/entity wall remains 256 — not raised in this experiment.",
        "",
        "p95*/p99* = peak_of_window_percentiles (<=120-tick server ring).",
        "",
        "## Main table",
        "",
        "| Connected | Requested | Ramp ms | Drops | First drop | Drop phase | Tick mean | p99* | Max | Input Q | Session Q | Out MiB/s | Status |",
        "| --------: | --------: | ------: | ----: | ---------: | ---------- | --------: | ---: | --: | ------: | --------: | --------: | ------ |",
    ]
    for r in sorted(runs, key=lambda x: (x["peak_connected"], x["ramp_ms"])):
        lines.append(
            "| {c} | {req} | {ramp} | {d} | {fd} | {ph} | {mean} | {p99} | {mx} | {iq} | {sq} | {out} | {st} |".format(
                c=r["peak_connected"],
                req=r["requested_bots"],
                ramp=r["ramp_ms"],
                d=r["input_handoff_dropped_total"],
                fd=_fmt(r.get("first_drop_secs")),
                ph=r.get("drop_phase"),
                mean=_fmt(r.get("server_tick_work_mean_ms")),
                p99=_fmt(r.get("server_tick_work_p99_ms")),
                mx=_fmt(r.get("server_tick_work_max_ms")),
                iq=r.get("server_input_queue_max"),
                sq=r.get("server_session_queue_max"),
                out=_fmt(r.get("outbound_mib_per_sec")),
                st=r.get("run_status"),
            )
        )

    lim = payload["limitation"]
    lines.extend(
        [
            "",
            "## Classification",
            "",
            f"**{lim['classification']}** — {lim['note']}",
            "",
            "## Answers",
            "",
            f"- A (100): slow eliminates={lim['answers']['A_100_slow_eliminates']}, all_zero={lim['answers']['A_100_all_zero']}",
            f"- B (200): slow eliminates={lim['answers']['B_200_slow_eliminates']}, all_zero={lim['answers']['B_200_all_zero']}",
            f"- C (256): slow eliminates={lim['answers']['C_256_slow_eliminates']}, all_zero={lim['answers']['C_256_all_zero']}",
            f"- D drop phases: {lim['answers']['D_drop_phases']}",
            f"- E queue recovery input={lim['answers']['E_queues_recover']['input']} session={lim['answers']['E_queues_recover']['session']}",
            f"- F slow-ramp sim within 30 Hz budget: {lim['answers']['F_steady_sim_budget_ok']}",
            f"- G spikes correlate with snapshot build/encode: {lim['answers']['G_spikes_correlate_snapshot']}",
            "",
            "## Network scaling note",
            "",
            "Outbound vs connected growth supports O(N^2)-like **network fan-out** "
            "(each of N sessions receives a snapshot of N entities). "
            "This does **not** by itself prove O(N^2) snapshot **CPU** construction; "
            "use tick_work_spike events + snapshot_*_max_ms for that correlation.",
            "",
            "## Graphs",
            "",
        ]
    )
    for g in payload.get("graphs", []):
        lines.append(f"- [`graphs/{g}`](graphs/{g})")
    lines.extend(["", "## Source runs", ""])
    for r in runs:
        lines.append(f"- `{r['run_dir']}`")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_dirs", nargs="+")
    parser.add_argument("--out", required=True, help="capacity/<stamp>/ramp_isolation")
    parser.add_argument("--workspace", default=".")
    args = parser.parse_args()
    workspace = Path(args.workspace).resolve()
    out = Path(args.out)
    if not out.is_absolute():
        out = workspace / out
    out.mkdir(parents=True, exist_ok=True)

    runs = []
    for raw in args.run_dirs:
        p = Path(raw)
        if not p.is_absolute():
            p = workspace / p
        if not p.is_dir():
            raise SystemExit(f"missing {p}")
        runs.append(analyze_run(p))

    runs.sort(key=lambda r: (r["peak_connected"], r["ramp_ms"], r["requested_bots"]))
    limitation = classify_limitation(runs)
    graphs = try_graphs(out, runs)
    payload = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "scope": "ramp_vs_steady_state_isolation",
        "not_production_capacity": True,
        "admission_cap": 256,
        "primary_xaxis": "peak_connected",
        "runs": runs,
        "limitation": limitation,
        "graphs": graphs,
    }
    # Strip bulky time_series from JSON file? Keep for graphs already done; shrink JSON.
    slim_runs = []
    for r in runs:
        rr = dict(r)
        rr.pop("time_series", None)
        slim_runs.append(rr)
    payload_slim = dict(payload)
    payload_slim["runs"] = slim_runs

    (out / "ramp_summary.json").write_text(
        json.dumps(payload_slim, indent=2), encoding="utf-8"
    )
    write_md(out / "ramp_summary.md", payload_slim)
    print(f"Wrote {out}")
    print("  ramp_summary.json")
    print("  ramp_summary.md")
    for g in graphs:
        print(f"  graphs/{g}")
    print(f"Classification: {limitation['classification']}")


if __name__ == "__main__":
    main()
