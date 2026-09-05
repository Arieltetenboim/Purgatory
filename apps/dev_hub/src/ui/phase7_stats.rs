//! Phase 7 statistics — displays frozen Phase 7.8 gate artifacts (no recompute).

use eframe::egui::{self, Color32, RichText};
use purgatory_dev_runtime::{
    HubCommand, HubSnapshot, Phase78CellBrief, Phase78GateBrief, Phase78ReadStatus,
};

use crate::theme;
use crate::ui::layout::{self, PageOutcome, card};

fn fmt_opt_f64(v: Option<f64>, digits: usize) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{x:.digits$}"),
        _ => "—".into(),
    }
}

fn fmt_opt_u64(v: Option<u64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "—".into(),
    }
}

fn fmt_opt_i64(v: Option<i64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "—".into(),
    }
}

fn fmt_opt_str(v: Option<&str>) -> String {
    match v {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => "—".into(),
    }
}

fn verdict_color(verdict: &str) -> Color32 {
    match verdict.to_ascii_uppercase().as_str() {
        "GREEN" => theme::success(),
        "YELLOW" => Color32::from_rgb(220, 175, 50),
        "RED" => theme::destructive(),
        "INVALID" => Color32::from_rgb(180, 120, 220),
        _ => theme::muted(),
    }
}

fn finding_class_color(class: &str) -> Color32 {
    match class {
        "HARNESS" => Color32::from_rgb(220, 175, 50),
        "SERVER" => theme::destructive(),
        "REGRESSION" => Color32::from_rgb(210, 140, 60),
        "WARN" => Color32::from_rgb(200, 160, 70),
        _ => theme::muted(),
    }
}

fn primary_cell(gate: &Phase78GateBrief) -> Option<&Phase78CellBrief> {
    gate.cells
        .iter()
        .find(|c| c.cell_id.contains("standard"))
        .or_else(|| gate.cells.first())
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::muted()));
        ui.label(RichText::new(value).color(theme::body()).strong());
    });
}

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> PageOutcome {
    let mut out = PageOutcome::none();
    layout::page_header(
        ui,
        "Phase 7 Statistics",
        "Read-only view of the frozen Phase 7.8 production gate artifacts. Does not recompute capacity metrics.",
    );

    let gate = &snap.phase78_gate;
    let mut gate_cmd: Option<HubCommand> = None;
    card(ui, "Gate summary", |ui| {
        match gate.status {
            Phase78ReadStatus::Missing => {
                ui.colored_label(
                    theme::muted(),
                    gate.read_error.as_deref().unwrap_or(
                        "no artifact — run Settings → PHASE 7.8 GATE (or scripts/phase_78_gate.ps1).",
                    ),
                );
                return;
            }
            Phase78ReadStatus::ParseError => {
                ui.colored_label(
                    theme::destructive(),
                    gate.read_error
                        .as_deref()
                        .unwrap_or("gate summary exists but failed to parse"),
                );
                ui.colored_label(
                    theme::muted(),
                    "File is present under logs/load/capacity_78/gate_*/ but could not be read as JSON.",
                );
                return;
            }
            Phase78ReadStatus::Ok => {}
        }
        if !gate.available {
            ui.colored_label(theme::muted(), "unavailable");
            return;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Verdict").color(theme::muted()));
            ui.label(
                RichText::new(gate.verdict.to_ascii_uppercase())
                    .color(verdict_color(&gate.verdict))
                    .strong()
                    .size(18.0),
            );
        });
        metric_row(ui, "Canonical run", &fmt_opt_str(Some(gate.stamp.as_str())));
        metric_row(ui, "Run root", &fmt_opt_str(Some(gate.run_root.as_str())));
        metric_row(ui, "Unit budget", &gate.unit_budget_status);
        metric_row(
            ui,
            "Baseline compared",
            if gate.baseline_compared { "yes" } else { "no" },
        );
        if let Some(path) = &gate.baseline_path {
            metric_row(ui, "Baseline path", path);
        }
        if !gate.verdict_explanation.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(&gate.verdict_explanation)
                    .color(Color32::from_rgb(220, 175, 50))
                    .italics(),
            );
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("RUN PHASE 7.8 GATE").clicked() {
                gate_cmd = Some(HubCommand::Phase78Gate);
            }
            if ui.button("OPEN LOAD LOGS").clicked() {
                gate_cmd = Some(HubCommand::OpenLoadLogs);
            }
        });
    });
    if let Some(c) = gate_cmd {
        out.command = Some(c);
    }
    ui.add_space(theme::SECTION_GAP);

    let cell = primary_cell(gate);
    card(ui, "Compact metrics (canonical / primary cell)", |ui| {
        let Some(c) = cell else {
            ui.colored_label(theme::muted(), "unavailable");
            return;
        };
        metric_row(ui, "Cell", &c.cell_id);
        metric_row(ui, "Cell status", &c.status);
        metric_row(
            ui,
            "Tick p95 / p99 / max (ms)",
            &format!(
                "{} / {} / {}",
                fmt_opt_f64(c.tick_p95, 2),
                fmt_opt_f64(c.tick_p99, 2),
                fmt_opt_f64(c.tick_max, 2)
            ),
        );
        metric_row(ui, "Tick util %", &fmt_opt_f64(c.tick_util, 1));
        metric_row(ui, "Dominant owner", &fmt_opt_str(c.dominant.as_deref()));
        metric_row(ui, "Server CPU mid %", &fmt_opt_f64(c.server_cpu_mid, 1));
        metric_row(
            ui,
            "Server RSS MB (Δ)",
            &format!(
                "{} ({})",
                fmt_opt_f64(c.server_rss_mb, 1),
                fmt_opt_f64(c.server_rss_delta_mb, 1)
            ),
        );
        metric_row(
            ui,
            "Replication bytes out /s",
            &fmt_opt_f64(c.bytes_out_per_sec, 0),
        );
        metric_row(
            ui,
            "Writer queue depth max / push fail",
            &format!(
                "{} / {}",
                fmt_opt_u64(c.writer_queue_depth_max),
                fmt_opt_u64(c.writer_queue_push_fail)
            ),
        );
        metric_row(
            ui,
            "Clients requested / peak active",
            &format!(
                "{} / {}",
                fmt_opt_u64(c.spawn_requested),
                fmt_opt_u64(c.peak_active)
            ),
        );
        metric_row(
            ui,
            "Workload attainment %",
            &fmt_opt_f64(c.attainment_pct, 1),
        );
        metric_row(
            ui,
            "Harness exit / snapshot_starvation",
            &format!(
                "{} / {}",
                fmt_opt_i64(c.harness_exit),
                fmt_opt_u64(c.snapshot_starvation)
            ),
        );
        metric_row(ui, "Deaths total", &fmt_opt_u64(c.deaths_total));
    });
    ui.add_space(theme::SECTION_GAP);

    card(ui, "Findings (HARNESS vs SERVER)", |ui| {
        if !gate.available {
            ui.colored_label(theme::muted(), "unavailable");
            return;
        }
        ui.label(
            RichText::new(
                "HARNESS WARN must not be read as SERVER WARN. Codes starting with HARNESS_ / attainment / funnel are harness class.",
            )
            .color(theme::muted())
            .small(),
        );
        ui.add_space(4.0);
        let mut any = false;
        for c in &gate.cells {
            for f in &c.findings {
                any = true;
                let class = f.class_label();
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("[{class}]"))
                            .color(finding_class_color(class))
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("{} {}", f.severity, f.code)).color(theme::body()),
                    );
                    ui.label(RichText::new(&c.cell_id).color(theme::muted()).small());
                    ui.label(RichText::new(&f.message).color(theme::muted()));
                });
            }
        }
        if !any {
            ui.colored_label(theme::success(), "No findings recorded in summary.");
        }
    });
    ui.add_space(theme::SECTION_GAP);

    for c in &gate.cells {
        egui::CollapsingHeader::new(format!("Cell detail — {}", c.cell_id))
            .default_open(c.cell_id.contains("standard") || c.cell_id.contains("soak"))
            .show(ui, |ui| {
                expand_tick(ui, c);
                expand_replication(ui, c);
                expand_network(ui, c);
                expand_cpu_mem(ui, c);
                expand_workload(ui, c);
                expand_harness(ui, c);
                expand_findings(ui, c);
            });
        ui.add_space(4.0);
    }

    egui::CollapsingHeader::new("Gate thresholds / baseline / soak notes")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Thresholds: scripts/phase_78_thresholds.json. Baseline: logs/load/capacity_78/baseline/baseline.json. Soak cell uses hotspot+NPC (not MixedRuntime) to avoid harness portal-role abort. See docs/PHASE_78_REPORT.md.",
                )
                .color(theme::muted()),
            );
            if gate.baseline_compared {
                metric_row(
                    ui,
                    "Baseline",
                    gate.baseline_path.as_deref().unwrap_or("compared"),
                );
            } else {
                metric_row(ui, "Baseline", "not compared / unavailable");
            }
            let soak = gate.cells.iter().find(|c| c.cell_id.contains("soak"));
            match soak {
                Some(s) => {
                    metric_row(ui, "Soak cell", &s.cell_id);
                    metric_row(ui, "Soak status", &s.status);
                    metric_row(ui, "Soak tick p99", &fmt_opt_f64(s.tick_p99, 2));
                    metric_row(ui, "Soak RSS Δ MB", &fmt_opt_f64(s.server_rss_delta_mb, 1));
                }
                None => metric_row(ui, "Soak", "unavailable"),
            }
        });

    out
}

fn expand_tick(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("Tick / owner attribution")
        .id_salt(format!("tick_{}", c.cell_id))
        .show(ui, |ui| {
            metric_row(ui, "p95 ms", &fmt_opt_f64(c.tick_p95, 2));
            metric_row(ui, "p99 ms", &fmt_opt_f64(c.tick_p99, 2));
            metric_row(ui, "max ms", &fmt_opt_f64(c.tick_max, 2));
            metric_row(ui, "util %", &fmt_opt_f64(c.tick_util, 1));
            metric_row(ui, "dominant", &fmt_opt_str(c.dominant.as_deref()));
        });
}

fn expand_replication(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("Replication")
        .id_salt(format!("repl_{}", c.cell_id))
        .show(ui, |ui| {
            metric_row(
                ui,
                "bytes out /s",
                &fmt_opt_f64(c.bytes_out_per_sec, 0),
            );
            ui.colored_label(
                theme::muted(),
                "Detailed replication_fanout.json fields are not duplicated here; use the cell artifact directory.",
            );
        });
}

fn expand_network(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("Network / backpressure")
        .id_salt(format!("net_{}", c.cell_id))
        .show(ui, |ui| {
            metric_row(
                ui,
                "writer queue depth max",
                &fmt_opt_u64(c.writer_queue_depth_max),
            );
            metric_row(
                ui,
                "writer queue push fail",
                &fmt_opt_u64(c.writer_queue_push_fail),
            );
        });
}

fn expand_cpu_mem(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("CPU / memory")
        .id_salt(format!("cpu_{}", c.cell_id))
        .show(ui, |ui| {
            metric_row(ui, "CPU mid %", &fmt_opt_f64(c.server_cpu_mid, 1));
            metric_row(ui, "RSS MB", &fmt_opt_f64(c.server_rss_mb, 1));
            metric_row(ui, "RSS Δ MB", &fmt_opt_f64(c.server_rss_delta_mb, 1));
        });
}

fn expand_workload(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("Gameplay workload")
        .id_salt(format!("wl_{}", c.cell_id))
        .show(ui, |ui| {
            metric_row(ui, "requested", &fmt_opt_u64(c.spawn_requested));
            metric_row(ui, "peak active", &fmt_opt_u64(c.peak_active));
            metric_row(ui, "attainment %", &fmt_opt_f64(c.attainment_pct, 1));
            metric_row(ui, "deaths", &fmt_opt_u64(c.deaths_total));
        });
}

fn expand_harness(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("Harness")
        .id_salt(format!("harness_{}", c.cell_id))
        .show(ui, |ui| {
            metric_row(ui, "exit", &fmt_opt_i64(c.harness_exit));
            metric_row(
                ui,
                "snapshot_starvation samples",
                &fmt_opt_u64(c.snapshot_starvation),
            );
            ui.colored_label(
                Color32::from_rgb(220, 175, 50),
                "snapshot_starvation is HARNESS class — client bot missed snapshots; not a server tick fail by itself.",
            );
        });
}

fn expand_findings(ui: &mut egui::Ui, c: &Phase78CellBrief) {
    egui::CollapsingHeader::new("Cell findings")
        .id_salt(format!("find_{}", c.cell_id))
        .show(ui, |ui| {
            if c.findings.is_empty() {
                ui.colored_label(theme::muted(), "none");
                return;
            }
            for f in &c.findings {
                let class = f.class_label();
                ui.label(
                    RichText::new(format!(
                        "[{class}] {} {} — {}",
                        f.severity, f.code, f.message
                    ))
                    .color(finding_class_color(class)),
                );
            }
        });
}
