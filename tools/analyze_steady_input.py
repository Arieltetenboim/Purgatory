#!/usr/bin/env python3
"""Phase 5.7 steady-state input handoff isolation report."""

from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path
from typing import Any


def f(v: Any, default: float | None = None) -> float | None:
    if v is None or v == "":
        return default
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def i(v: Any, default: int = 0) -> int:
    x = f(v)
    return int(x) if x is not None else default


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def load_events(path: Path) -> list[dict]:
    if not path.exists():
        return []
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        out.append(json.loads(line))
    return out


def load_metrics(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with path.open(encoding="utf-8", newline="") as fh:
        return list(csv.DictReader(fh))


def phase_slices(
    rows: list[dict], activation_ts: float | None
) -> dict[str, list[dict]]:
    if activation_ts is None:
        return {"pre": rows, "first10": [], "steady": rows}
    pre, first10, steady = [], [], []
    for r in rows:
        t = f(r.get("elapsed_secs"), 0.0) or 0.0
        if t < activation_ts:
            pre.append(r)
        elif t < activation_ts + 10.0:
            first10.append(r)
        else:
            steady.append(r)
    return {"pre": pre, "first10": first10, "steady": steady}


def slice_stats(rows: list[dict], activation_ts: float | None, label: str) -> dict:
    if not rows:
        return {
            "label": label,
            "samples": 0,
            "handoff_start": None,
            "handoff_end": None,
            "handoff_delta": 0,
            "input_q_max": None,
            "session_q_max": None,
            "tick_mean_avg": None,
            "tick_max_peak": None,
            "cmds_start": None,
            "cmds_end": None,
        }
    handoffs = [i(r.get("server_input_handoff_dropped")) for r in rows]
    # Prefer dedicated column; fall back to empty→0 for older CSVs.
    qs = [i(r.get("server_input_queue_max")) for r in rows]
    sess = [i(r.get("server_session_queue_max")) for r in rows]
    means = [f(r.get("server_tick_work_mean_ms")) for r in rows]
    maxes = [f(r.get("server_tick_work_max_ms")) for r in rows]
    means_n = [x for x in means if x is not None]
    maxes_n = [x for x in maxes if x is not None]
    return {
        "label": label,
        "samples": len(rows),
        "handoff_start": handoffs[0],
        "handoff_end": handoffs[-1],
        "handoff_delta": max(0, handoffs[-1] - handoffs[0]),
        "input_q_max": max(qs) if qs else None,
        "session_q_max": max(sess) if sess else None,
        "tick_mean_avg": sum(means_n) / len(means_n) if means_n else None,
        "tick_max_peak": max(maxes_n) if maxes_n else None,
        "cmds_start": i(rows[0].get("commands_sent")),
        "cmds_end": i(rows[-1].get("commands_sent")),
        "activation_ts": activation_ts,
    }


def analyze_run(run_dir: Path) -> dict:
    summary = load_json(run_dir / "run_summary.json")
    config = load_json(run_dir / "config.json")
    events = load_events(run_dir / "events.ndjson")
    rows = load_metrics(run_dir / "metrics.csv")

    activation_ts = None
    first_drop_ts = None
    first_drop_phase = "none"
    spikes = []
    for ev in events:
        kind = ev.get("event")
        ts = f(ev.get("ts"))
        if kind == "input_activation":
            activation_ts = ts
        if kind in ("queue_overflow", "input_handoff_drop_steady") and first_drop_ts is None:
            if i(ev.get("input_handoff_dropped")) > 0 or i(ev.get("input_handoff_delta")) > 0:
                first_drop_ts = ts
        if kind == "tick_work_spike":
            spikes.append(ev)

    if activation_ts is not None and first_drop_ts is not None:
        if first_drop_ts < activation_ts:
            first_drop_phase = "pre_input"
        elif first_drop_ts < activation_ts + 10.0:
            first_drop_phase = "first_10s_after_activation"
        else:
            first_drop_phase = "steady_state_input"
    elif first_drop_ts is not None:
        first_drop_phase = "unknown"

    slices = phase_slices(rows, activation_ts)
    handoff_total = i(summary.get("input_handoff_dropped_total"))
    if handoff_total == 0 and rows:
        handoff_total = i(rows[-1].get("server_input_handoff_dropped"))

    spike_corr = []
    for sp in spikes:
        spike_corr.append(
            {
                "ts": f(sp.get("ts")),
                "tick_work_max_ms": f(sp.get("tick_work_max_ms")),
                "snapshot_build_time_max_ms": f(sp.get("snapshot_build_time_max_ms")),
                "snapshot_encode_time_max_ms": f(sp.get("snapshot_encode_time_max_ms")),
                "input_queue_current": i(sp.get("input_queue_current")),
                "input_handoff_delta": i(sp.get("input_handoff_delta")),
                "input_active": sp.get("input_active"),
            }
        )

    return {
        "run_dir": run_dir.name,
        "requested_bots": i(config.get("count") or summary.get("requested_bots")),
        "peak_connected": i(summary.get("peak_connected")),
        "scenario": config.get("scenario"),
        "seed": config.get("seed"),
        "quiet_secs": config.get("quiet_secs"),
        "status": summary.get("run_status"),
        "reasons": summary.get("status_reasons") or [],
        "input_activation_secs": activation_ts,
        "input_handoff_dropped_total": handoff_total,
        "first_drop_ts": first_drop_ts,
        "first_drop_phase": first_drop_phase,
        "tick_mean": f(summary.get("server_tick_work_mean_ms")),
        "tick_p99": f(summary.get("server_tick_work_p99_ms")),
        "tick_max": f(summary.get("server_tick_work_max_ms")),
        "tick_overruns": i(summary.get("server_tick_overruns_total")),
        "input_q_max": i(summary.get("server_input_queue_max")),
        "session_q_max": i(summary.get("server_session_queue_max")),
        "unexpected_disconnects": i(summary.get("unexpected_disconnects")),
        "encode_failures": i(summary.get("encode_failures")),
        "memory_peak_mb": f(summary.get("server_memory_peak_mb")),
        "memory_end_mb": f(summary.get("server_memory_end_mb")),
        "harness_memory_peak_mb": f(summary.get("harness_memory_peak_mb")),
        "phases": {
            "pre_input": slice_stats(slices["pre"], activation_ts, "pre_input"),
            "first_10s": slice_stats(slices["first10"], activation_ts, "first_10s"),
            "steady_input": slice_stats(slices["steady"], activation_ts, "steady_input"),
        },
        "spike_events": spike_corr,
        "metrics_rows": rows,
    }


def try_plot(out: Path, runs: list[dict]) -> list[str]:
    files = []
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except Exception:
        return files

    g = out / "graphs"
    g.mkdir(parents=True, exist_ok=True)

    xs = [r["peak_connected"] for r in runs]
    drops = [r["input_handoff_dropped_total"] for r in runs]
    fig, ax = plt.subplots(figsize=(7, 4))
    ax.plot(xs, drops, "o-", color="#b33")
    ax.set_xlabel("peak connected")
    ax.set_ylabel("input_handoff_dropped total")
    ax.set_title("Steady-state handoff drops vs connected")
    ax.grid(True, alpha=0.3)
    p = g / "handoff_vs_connected.png"
    fig.tight_layout()
    fig.savefig(p)
    plt.close(fig)
    files.append(str(p.name))

    fig, ax = plt.subplots(figsize=(7, 4))
    ax.plot(xs, [r["input_q_max"] for r in runs], "o-", label="input Q max")
    ax.plot(xs, [r["session_q_max"] for r in runs], "s-", label="session Q max")
    ax.set_xlabel("peak connected")
    ax.set_ylabel("queue peak")
    ax.legend()
    ax.grid(True, alpha=0.3)
    p = g / "queues_vs_connected.png"
    fig.tight_layout()
    fig.savefig(p)
    plt.close(fig)
    files.append(str(p.name))

    fig, ax = plt.subplots(figsize=(7, 4))
    ax.plot(xs, [r["tick_mean"] or 0 for r in runs], "o-", label="tick mean")
    ax.plot(xs, [r["tick_p99"] or 0 for r in runs], "s-", label="p99*")
    ax.axhline(33.333, color="gray", ls="--", label="33.3 ms budget")
    ax.set_xlabel("peak connected")
    ax.set_ylabel("ms")
    ax.legend()
    ax.grid(True, alpha=0.3)
    p = g / "tick_vs_connected.png"
    fig.tight_layout()
    fig.savefig(p)
    plt.close(fig)
    files.append(str(p.name))

    # Overlay time series from the worst run.
    worst = max(runs, key=lambda r: r["input_handoff_dropped_total"])
    rows = worst.get("metrics_rows") or []
    if rows:
        ts = [f(r.get("elapsed_secs"), 0) or 0 for r in rows]
        hand = [i(r.get("server_input_handoff_dropped")) for r in rows]
        iq = [i(r.get("server_input_queue_current")) for r in rows]
        tw = [f(r.get("server_tick_work_max_ms"), 0) or 0 for r in rows]
        sb = [f(r.get("snapshot_build_time_max_ms"), 0) or 0 for r in rows]
        act = worst.get("input_activation_secs")
        fig, axes = plt.subplots(3, 1, figsize=(9, 8), sharex=True)
        axes[0].plot(ts, hand, color="#b33")
        axes[0].set_ylabel("handoff dropped")
        axes[1].plot(ts, iq, label="input Q")
        axes[1].plot(ts, tw, label="tick max ms")
        axes[1].legend(loc="upper right", fontsize=8)
        axes[1].set_ylabel("queue / tick")
        axes[2].plot(ts, sb, color="#36a", label="snap build max ms")
        axes[2].legend(loc="upper right", fontsize=8)
        axes[2].set_ylabel("snapshot")
        axes[2].set_xlabel("elapsed secs")
        if act is not None:
            for ax in axes:
                ax.axvline(act, color="green", ls="--", alpha=0.7)
        fig.suptitle(f"Time series — {worst['run_dir']} (peak={worst['peak_connected']})")
        fig.tight_layout()
        p = g / "timeseries_worst.png"
        fig.savefig(p)
        plt.close(fig)
        files.append(str(p.name))

    return files


def classify(runs: list[dict]) -> dict:
    any_drops = any(r["input_handoff_dropped_total"] > 0 for r in runs)
    pre_drops = any(
        (r["phases"]["pre_input"].get("handoff_delta") or 0) > 0 for r in runs
    )
    post_drops = any(
        (r["phases"]["first_10s"].get("handoff_delta") or 0)
        + (r["phases"]["steady_input"].get("handoff_delta") or 0)
        > 0
        for r in runs
    )
    q_recover = []
    for r in runs:
        pre_q = r["phases"]["pre_input"].get("input_q_max") or 0
        st_q = r["phases"]["steady_input"].get("input_q_max") or 0
        # Heuristic: recover if steady peak is not runaway vs first10
        f10 = r["phases"]["first_10s"].get("input_q_max") or 0
        q_recover.append(st_q <= max(f10, pre_q, 1) * 2)

    spike_with_drop = 0
    spike_with_snap = 0
    for r in runs:
        for sp in r["spike_events"]:
            if (sp.get("snapshot_build_time_max_ms") or 0) >= 1.0 or (
                sp.get("snapshot_encode_time_max_ms") or 0
            ) >= 1.0:
                spike_with_snap += 1
            if (sp.get("input_handoff_delta") or 0) > 0:
                spike_with_drop += 1

    if not any_drops:
        failure_class = "NONE_OBSERVED"
        note = "No input_handoff_dropped under steady-input matrix."
    elif pre_drops and not post_drops:
        failure_class = "RAMP_OR_CONNECT_ONLY"
        note = "Drops only before input activation (unexpected for this design)."
    elif post_drops and not pre_drops:
        # Distinguish throughput vs contention via queue persistence + spike corr
        persistent = any(
            (r["phases"]["steady_input"].get("handoff_delta") or 0) > 100 for r in runs
        )
        if spike_with_drop > 0 and spike_with_snap > 0 and persistent:
            failure_class = "E_MIXED"
            note = (
                "Drops after input activation; correlate with snapshot/tick spikes "
                "and continue in steady-state (contention + sustained pressure)."
            )
        elif spike_with_drop > 0 and all(q_recover):
            failure_class = "B_CONTENTION_STARVATION"
            note = "Drops cluster with tick/snapshot spikes; queues recover."
        elif persistent:
            failure_class = "A_SUSTAINED_THROUGHPUT"
            note = "Handoff drops continue through steady-state input."
        else:
            failure_class = "E_MIXED"
            note = "Post-activation drops with mixed queue/spike signals."
    else:
        failure_class = "E_MIXED"
        note = "Drops in multiple phases."

    return {
        "failure_class": failure_class,
        "note": note,
        "any_drops": any_drops,
        "pre_input_drops": pre_drops,
        "post_activation_drops": post_drops,
        "spike_events_with_handoff_delta": spike_with_drop,
        "spike_events_with_snapshot_ms": spike_with_snap,
        "semantic_note": (
            "input_handoff_dropped means try_send on the global input mpsc failed: "
            "a real sequenced InputCommand never reached SessionInput. Because Gap "
            "rejects non-contiguous sequences, one drop soft-bricks further Accepts "
            "for that session until epoch/HeldCancel. move_axis/down_held are "
            "continuous (latest-wins once queued); jump_pressed is an edge — drop "
            "can lose a jump. Not harness-misreported: counter increments only on "
            "failed handoff after QUIC decode/rate-allow."
        ),
    }


def write_md(path: Path, payload: dict) -> None:
    runs = payload["runs"]
    lim = payload["analysis"]
    lines = [
        "# PURGATORY Phase 5.7 — Steady-state input handoff isolation",
        "",
        f"Tag: `{payload.get('tag')}`",
        "",
        "## Semantics",
        "",
        lim["semantic_note"],
        "",
        "## Matrix",
        "",
        "| Connected | Drops | First drop | Phase | Tick mean | p99* | Max | In Q | Sess Q | Status |",
        "| --------: | ----: | ---------: | ----- | --------: | ---: | --: | ---: | -----: | ------ |",
    ]
    for r in runs:
        lines.append(
            "| {c} | {d} | {fd} | {ph} | {tm} | {p99} | {mx} | {iq} | {sq} | {st} |".format(
                c=r["peak_connected"],
                d=r["input_handoff_dropped_total"],
                fd="—" if r["first_drop_ts"] is None else f"{r['first_drop_ts']:.1f}s",
                ph=r["first_drop_phase"],
                tm="—" if r["tick_mean"] is None else f"{r['tick_mean']:.2f}",
                p99="—" if r["tick_p99"] is None else f"{r['tick_p99']:.1f}",
                mx="—" if r["tick_max"] is None else f"{r['tick_max']:.1f}",
                iq=r["input_q_max"],
                sq=r["session_q_max"],
                st=r["status"],
            )
        )
    lines.extend(
        [
            "",
            "## Phase deltas (handoff)",
            "",
            "| Connected | Pre Δ | First10 Δ | Steady Δ |",
            "| --------: | ----: | --------: | -------: |",
        ]
    )
    for r in runs:
        lines.append(
            "| {c} | {a} | {b} | {d} |".format(
                c=r["peak_connected"],
                a=r["phases"]["pre_input"]["handoff_delta"],
                b=r["phases"]["first_10s"]["handoff_delta"],
                d=r["phases"]["steady_input"]["handoff_delta"],
            )
        )
    lines.extend(
        [
            "",
            "## Classification",
            "",
            f"**{lim['failure_class']}** — {lim['note']}",
            "",
            f"Graphs: {', '.join(payload.get('graphs') or []) or '(none)'}",
            "",
        ]
    )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs-file", required=True, help="text file of run dir names")
    ap.add_argument("--logs", default="logs/load")
    ap.add_argument("--out", required=True)
    ap.add_argument("--tag", default="before")
    args = ap.parse_args()

    logs = Path(args.logs)
    names = [
        ln.strip()
        for ln in Path(args.runs_file).read_text(encoding="utf-8").splitlines()
        if ln.strip() and not ln.strip().startswith("#")
    ]
    runs = []
    for name in names:
        d = logs / name
        row = analyze_run(d)
        # Drop bulky metrics from JSON payload after plots
        runs.append(row)

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    graphs = try_plot(out, runs)
    for r in runs:
        r.pop("metrics_rows", None)

    analysis = classify(runs)
    payload = {
        "tag": args.tag,
        "scope": "steady_state_input_handoff_isolation",
        "runs": runs,
        "analysis": analysis,
        "graphs": graphs,
    }
    (out / "steady_summary.json").write_text(
        json.dumps(payload, indent=2), encoding="utf-8"
    )
    write_md(out / "steady_summary.md", payload)
    print(f"Wrote {out / 'steady_summary.md'}")
    print(f"class={analysis['failure_class']}")


if __name__ == "__main__":
    main()
