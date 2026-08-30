#!/usr/bin/env python3
"""Regression tests for analyze_load_run aggregation and charts."""

from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

import analyze_load_run as alr


def _write_csv(path: Path, rows: list[dict], fieldnames: list[str]) -> None:
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        for row in rows:
            w.writerow(row)


class AggregateTests(unittest.TestCase):
    def test_long_sequence_not_all_none(self) -> None:
        rows = []
        for i in range(3000):
            if i % 20 == 0:
                rows.append(
                    {
                        "elapsed_secs": str(i),
                        "connected_bots": "100",
                        "server_tick_work_mean_ms": "",
                        "server_tick_work_p95_ms": "",
                        "server_tick_work_p99_ms": "",
                        "server_tick_work_max_ms": "",
                        "server_tick_overruns_total": "",
                        "server_memory_mb": "",
                        "server_input_queue_max": "",
                        "server_session_queue_max": "",
                        "server_active_sessions": "",
                    }
                )
            else:
                rows.append(
                    {
                        "elapsed_secs": str(i),
                        "connected_bots": "100",
                        "server_tick_work_mean_ms": "1.5",
                        "server_tick_work_p95_ms": "2.0",
                        "server_tick_work_p99_ms": "3.0",
                        "server_tick_work_max_ms": "4.0",
                        "server_tick_overruns_total": str(i // 100),
                        "server_memory_mb": "200.0",
                        "server_input_queue_max": "2",
                        "server_session_queue_max": "1",
                        "server_active_sessions": "100",
                    }
                )
        agg = alr.aggregate_from_csv(rows)
        self.assertTrue(agg["server_metrics_ok"])
        self.assertIsNotNone(agg["server_tick_work_mean_ms"])
        self.assertIsNotNone(agg["server_tick_work_p95_ms"])
        self.assertIsNotNone(agg["server_tick_work_p99_ms"])
        self.assertIsNotNone(agg["server_tick_work_max_ms"])
        self.assertIsNotNone(agg["server_tick_overruns_total"])
        self.assertIsNotNone(agg["server_memory_peak_mb"])
        self.assertIsNotNone(agg["server_input_queue_max"])

    def test_enrich_fills_missing_summary_keys(self) -> None:
        summary = {
            "status": "Warn",
            "server_metrics_ok": True,
            "peak_connected": 100,
        }
        rows = [
            {
                "elapsed_secs": "1",
                "connected": "10",
                "server_tick_p99_ms": "5.0",
                "server_tick_overrun": "0",
                "server_active_sessions": "10",
            },
            {
                "elapsed_secs": "2",
                "connected": "10",
                "server_tick_p99_ms": "6.0",
                "server_tick_overrun": "1",
                "server_active_sessions": "10",
            },
        ]
        enriched = alr.enrich_summary(summary, rows)
        self.assertEqual(enriched["run_status"], "WARN")
        self.assertEqual(enriched["server_tick_work_p99_ms"], 6.0)
        self.assertEqual(enriched["server_tick_overruns_total"], 1)
        self.assertEqual(enriched["status_reasons"], [])

    def test_no_metrics(self) -> None:
        agg = alr.aggregate_from_csv([])
        self.assertFalse(agg["server_metrics_ok"])
        self.assertIsNone(agg["server_tick_work_mean_ms"])

    def test_partial_column_independence(self) -> None:
        rows = [
            {
                "elapsed_secs": "1",
                "server_tick_work_mean_ms": "2.0",
                "server_active_sessions": "1",
                "server_memory_mb": "",
                "server_input_queue_max": "4",
            }
        ]
        agg = alr.aggregate_from_csv(rows)
        self.assertEqual(agg["server_tick_work_mean_ms"], 2.0)
        self.assertIsNone(agg["server_memory_start_mb"])
        self.assertEqual(agg["server_input_queue_max"], 4)


class ChartTests(unittest.TestCase):
    def test_charts_from_fixture(self) -> None:
        try:
            import matplotlib  # noqa: F401
        except ImportError:
            self.skipTest("matplotlib not installed")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            graphs = root / "graphs"
            rows = []
            for i in range(120):
                rows.append(
                    {
                        "elapsed_secs": str(float(i)),
                        "connected_bots": "10",
                        "target_bots": "10",
                        "server_active_sessions": "10",
                        "server_tick_work_p50_ms": "0.5",
                        "server_tick_work_p95_ms": "1.0",
                        "server_tick_work_p99_ms": "1.5",
                        "server_tick_work_max_ms": "2.0",
                        "bot_scheduler_p99_ms": "33.0",
                        "server_bytes_in_per_sec": "1000",
                        "server_bytes_out_per_sec": "5000",
                        "server_memory_mb": "100",
                        "harness_memory_mb": "50",
                        "server_input_queue_current": "1",
                        "server_input_queue_max": "2",
                        "server_session_queue_max": "1",
                    }
                )
            created, reason = alr.try_charts(graphs, rows)
            self.assertIsNone(reason)
            self.assertIn("population.png", created)
            self.assertIn("tick_timing.png", created)
            self.assertIn("bandwidth.png", created)
            self.assertIn("memory.png", created)
            self.assertIn("queues.png", created)
            for name in created:
                self.assertTrue((graphs / name).is_file())


if __name__ == "__main__":
    # Allow importing sibling module when run from tools/
    unittest.main()
