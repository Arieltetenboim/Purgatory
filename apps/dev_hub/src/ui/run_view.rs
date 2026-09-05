//! Shared Validation / Performance run presentation (presenter only).

use eframe::egui::{self, RichText};
use purgatory_dev_runtime::{
    CapacityLiveSnapshot, HubCommand, HubSnapshot, MetricsSeries, RunSummaryBrief, SaturationClass,
    ServerState, TickOwnerId, ValidationLiveStatus, ValidationState,
};

use crate::theme;
use crate::ui::layout::{
    self, ChartSeries, PageOutcome, bounded_log_panel, format_mb, format_ms, format_secs, kv_row,
    metric_tile, status_pill,
};

pub struct RunViewModel<'a> {
    pub kind: &'static str,
    pub phase: ValidationState,
    pub reason: Option<&'a str>,
    pub active_label: Option<&'a str>,
    pub live: &'a ValidationLiveStatus,
    pub metrics: &'a MetricsSeries,
    pub log_lines: &'a [String],
    pub last_outcome: Option<ValidationState>,
    pub last_dir: Option<&'a std::path::Path>,
    pub summary: &'a RunSummaryBrief,
    pub server_state: ServerState,
    pub can_start: bool,
    pub can_stop: bool,
    pub start_disabled_hint: &'a str,
    pub capacity: &'a CapacityLiveSnapshot,
}

pub struct RunViewState {
    pub show_full_details: bool,
    pub chart_series: ChartSeries,
}

impl Default for RunViewState {
    fn default() -> Self {
        Self {
            show_full_details: false,
            chart_series: ChartSeries::Connected,
        }
    }
}

pub fn show_run(
    ui: &mut egui::Ui,
    model: &RunViewModel<'_>,
    state: &mut RunViewState,
    start_cmd: HubCommand,
    stop_cmd: HubCommand,
    extra_actions: impl FnOnce(&mut egui::Ui, &mut Option<HubCommand>),
) -> PageOutcome {
    let mut cmd = None;
    let active = model.phase.is_active();
    let terminal = model.last_outcome.is_some()
        || matches!(
            model.phase,
            ValidationState::Passed
                | ValidationState::Failed
                | ValidationState::Cancelled
                | ValidationState::OrchestrationFailed
        );

    layout::card(ui, "Run status", |ui| {
        ui.horizontal(|ui| {
            status_pill(ui, model.phase.as_str(), theme::outcome_color(model.phase));
            ui.label(model.kind);
            if let Some(label) = model.active_label {
                ui.colored_label(theme::muted(), label);
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let start_label = if model.kind == "Runtime Validation" {
                "START VALIDATION"
            } else {
                "START LOAD"
            };
            let stop_label = if model.kind == "Runtime Validation" {
                "STOP VALIDATION"
            } else {
                "STOP LOAD"
            };
            if ui
                .add_enabled(model.can_start, egui::Button::new(start_label))
                .clicked()
            {
                cmd = Some(start_cmd.clone());
            }
            if ui
                .add_enabled(model.can_stop, egui::Button::new(stop_label))
                .clicked()
            {
                cmd = Some(stop_cmd);
            }
            extra_actions(ui, &mut cmd);
        });
        if !model.can_start && !active {
            ui.colored_label(theme::muted(), model.start_disabled_hint);
        }
        if let Some(reason) = model.reason {
            ui.colored_label(theme::muted(), reason);
        }
        ui.horizontal(|ui| {
            kv_pair_inline(ui, "Server", model.server_state.as_str());
            if let Some(elapsed) = model.live.elapsed_secs {
                kv_pair_inline(ui, "Elapsed", &format_secs(elapsed));
            }
            if let Some(dur) = model.live.duration_secs {
                kv_pair_inline(ui, "Duration", &format_secs(dur as f64));
            }
        });
    });
    ui.add_space(theme::SECTION_GAP);

    if active || model.live.available {
        layout::card(ui, "Live summary", |ui| {
            if let Some(line) = &model.live.status_line {
                ui.label(RichText::new(line).strong());
                ui.add_space(4.0);
            } else if active {
                ui.colored_label(theme::muted(), "Waiting for live_status.json…");
            }
            ui.horizontal(|ui| {
                if let Some(v) = model.live.real_connected {
                    metric_tile(
                        ui,
                        "Real connected",
                        &format!(
                            "{v}/{}",
                            model
                                .live
                                .persistent_target
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "—".into())
                        ),
                    );
                    ui.add_space(16.0);
                }
                if let Some(v) = model.live.churn_connected {
                    metric_tile(ui, "Churn", &v.to_string());
                    ui.add_space(16.0);
                }
                if let Some(v) = model.live.failures {
                    metric_tile(ui, "Failures", &v.to_string());
                    ui.add_space(16.0);
                }
                if let Some(v) = model.live.portal_transitions {
                    metric_tile(
                        ui,
                        "Portal",
                        &format!(
                            "{v}/{}",
                            model
                                .live
                                .portal_attempts
                                .map(|a| a.to_string())
                                .unwrap_or_else(|| "—".into())
                        ),
                    );
                }
            });
            if let Some(fail) = &model.live.early_fail {
                ui.colored_label(
                    theme::outcome_color(ValidationState::Failed),
                    format!("Early fail: {fail}"),
                );
            }
        });
        ui.add_space(theme::SECTION_GAP);

        if model.capacity.available {
            show_capacity_strip(ui, model.capacity);
            ui.add_space(theme::SECTION_GAP);
        }

        layout::card(ui, "Live chart (metrics.csv)", |ui| {
            layout::metrics_chart(ui, "run_live_chart", model.metrics, &mut state.chart_series);
        });
        ui.add_space(theme::SECTION_GAP);

        layout::card(ui, "Harness output (load.log)", |ui| {
            if bounded_log_panel(
                ui,
                "run_live_log",
                model.log_lines,
                "(empty — harness output appears as the run progresses)",
            ) {
                cmd = Some(HubCommand::ClearLoadLog);
            }
        });
        ui.add_space(theme::SECTION_GAP);
    }

    if terminal || model.summary.available || model.last_dir.is_some() {
        let outcome = model.last_outcome.or(
            if !model.phase.is_active() && model.phase != ValidationState::Idle {
                Some(model.phase)
            } else {
                None
            },
        );
        layout::card(ui, "Result summary", |ui| {
            if let Some(outcome) = outcome {
                status_pill(ui, outcome_label(outcome), theme::outcome_color(outcome));
            } else if model.summary.available {
                status_pill(
                    ui,
                    &model.summary.run_status,
                    status_from_run_status(&model.summary.run_status),
                );
            } else {
                ui.colored_label(theme::muted(), "No completed result yet.");
            }
            ui.add_space(4.0);
            if model.summary.available {
                ui.horizontal(|ui| {
                    metric_tile(ui, "Elapsed", &format_secs(model.summary.elapsed_secs));
                    ui.add_space(12.0);
                    metric_tile(
                        ui,
                        "Peak connected",
                        &format!(
                            "{}/{}",
                            model.summary.peak_connected, model.summary.requested_bots
                        ),
                    );
                    ui.add_space(12.0);
                    if let Some(p95) = model.summary.tick_work_p95_ms {
                        metric_tile(ui, "Tick p95", &format_ms(p95));
                        ui.add_space(12.0);
                    }
                    if let Some(mem) = model.summary.memory_peak_mb {
                        metric_tile(ui, "Mem peak", &format_mb(mem));
                    }
                });
                if !model.summary.failure_class.is_empty() && model.summary.failure_class != "none"
                {
                    kv_row(ui, "Failure class", &model.summary.failure_class);
                }
                if let Some(first) = model.summary.reasons.first() {
                    kv_row(
                        ui,
                        "Primary reason",
                        format!("{} — {}", first.code, first.message),
                    );
                }
            }
            if let Some(dir) = model.last_dir {
                kv_row(ui, "Artifact", dir.display().to_string());
            }
            ui.add_space(4.0);
            ui.checkbox(&mut state.show_full_details, "Show full details");
            if state.show_full_details {
                ui.add_space(6.0);
                if let Some(c) = show_full_details(ui, model) {
                    cmd = Some(c);
                }
            }
        });
        if !active && model.metrics.available {
            ui.add_space(theme::SECTION_GAP);
            layout::card(ui, "Final chart", |ui| {
                layout::metrics_chart(
                    ui,
                    "run_final_chart",
                    model.metrics,
                    &mut state.chart_series,
                );
            });
        }
    }

    if let Some(c) = cmd {
        PageOutcome::command(c)
    } else {
        PageOutcome::none()
    }
}

fn show_full_details(ui: &mut egui::Ui, model: &RunViewModel<'_>) -> Option<HubCommand> {
    if model.capacity.available {
        show_capacity_owners(ui, model.capacity);
        ui.add_space(8.0);
    }
    if let Some(label) = model.active_label {
        kv_row(ui, "Configuration", label);
    }
    if model.summary.available {
        kv_row(ui, "Harness status", &model.summary.run_status);
        kv_row(
            ui,
            "Requested duration",
            format_secs(model.summary.requested_duration_secs as f64),
        );
        if let Some(preset) = &model.summary.preset {
            kv_row(ui, "Preset", preset);
        }
        if let Some(seed) = model.summary.seed {
            kv_row(ui, "Seed", seed.to_string());
        }
        kv_row(
            ui,
            "Unexpected disconnects",
            model.summary.unexpected_disconnects.to_string(),
        );
        kv_row(
            ui,
            "Overflow events",
            model.summary.overflow_events.to_string(),
        );
        kv_row(
            ui,
            "Admission refusals",
            model.summary.admission_refusals.to_string(),
        );
        if let Some(mean) = model.summary.tick_work_mean_ms {
            kv_row(ui, "Tick mean", format_ms(mean));
        }
        if let Some(max) = model.summary.tick_work_max_ms {
            kv_row(ui, "Tick max", format_ms(max));
        }
        if !model.summary.reasons.is_empty() {
            ui.add_space(4.0);
            ui.strong("Status reasons");
            for r in &model.summary.reasons {
                ui.label(format!("• {} — {}", r.code, r.message));
            }
        }
    } else {
        ui.colored_label(
            theme::muted(),
            "run_summary.json not available for this artifact.",
        );
    }
    ui.add_space(6.0);
    ui.strong("Harness log (bounded)");
    if bounded_log_panel(
        ui,
        "run_details_log",
        model.log_lines,
        "(no harness log lines)",
    ) {
        Some(HubCommand::ClearLoadLog)
    } else {
        None
    }
}

fn outcome_label(state: ValidationState) -> &'static str {
    match state {
        ValidationState::Passed => "PASSED",
        ValidationState::Failed => "FAILED",
        ValidationState::Cancelled => "CANCELLED",
        ValidationState::OrchestrationFailed => "ORCHESTRATION ERROR",
        other => other.as_str(),
    }
}

fn status_from_run_status(status: &str) -> egui::Color32 {
    let u = status.to_ascii_uppercase();
    if u.contains("PASS") {
        theme::outcome_color(ValidationState::Passed)
    } else if u.contains("CANCEL") {
        theme::outcome_color(ValidationState::Cancelled)
    } else if u.contains("FAIL") {
        theme::outcome_color(ValidationState::Failed)
    } else {
        theme::muted()
    }
}

fn kv_pair_inline(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.colored_label(theme::muted(), format!("{key}:"));
    ui.label(value);
    ui.separator();
}

pub fn validation_model<'a>(snap: &'a HubSnapshot) -> RunViewModel<'a> {
    RunViewModel {
        kind: "Runtime Validation",
        phase: snap.validation,
        reason: snap.validation_reason.as_deref(),
        active_label: snap.validation_active_label.as_deref(),
        live: &snap.validation_live,
        metrics: &snap.validation_metrics,
        log_lines: &snap.load_log_lines,
        last_outcome: snap.validation_last.outcome,
        last_dir: snap.validation_last.dir.as_deref(),
        summary: &snap.validation_last.summary,
        server_state: snap.server_state,
        can_start: snap.can_start_validation,
        can_stop: snap.can_stop_validation,
        start_disabled_hint: "Server must be Ready. Refuse if Load is active.",
        capacity: &snap.validation_capacity,
    }
}

pub fn load_model<'a>(snap: &'a HubSnapshot) -> RunViewModel<'a> {
    RunViewModel {
        kind: "Load / soak",
        phase: snap.load,
        reason: snap.load_reason.as_deref(),
        active_label: snap.load_active_label.as_deref(),
        live: &snap.load_live,
        metrics: &snap.load_metrics,
        log_lines: &snap.load_log_lines,
        last_outcome: snap.load_last.outcome,
        last_dir: snap.load_last.dir.as_deref(),
        summary: &snap.load_last.summary,
        server_state: snap.server_state,
        can_start: snap.can_start_load,
        can_stop: snap.can_stop_load,
        start_disabled_hint: "Server should be Ready (or Stopped for load-mode start).",
        capacity: &snap.load_capacity,
    }
}

fn show_capacity_strip(ui: &mut egui::Ui, cap: &CapacityLiveSnapshot) {
    layout::card(ui, "Capacity (heuristic)", |ui| {
        ui.horizontal(|ui| {
            metric_tile(ui, "Class", saturation_label(cap.saturation_class));
            ui.add_space(12.0);
            metric_tile(
                ui,
                "Owner",
                cap.dominant_owner.map(TickOwnerId::as_str).unwrap_or("—"),
            );
            ui.add_space(12.0);
            metric_tile(ui, "Tick p99", &format_ms(cap.tick_p99_ms));
            ui.add_space(12.0);
            metric_tile(ui, "Util", &format!("{:.0}%", cap.tick_utilization_pct));
            ui.add_space(12.0);
            metric_tile(
                ui,
                "CPU (1-core=100)",
                &format!("{:.0}%", cap.cpu_utilization_pct),
            );
        });
        ui.add_space(4.0);
        ui.colored_label(
            theme::muted(),
            format!(
                "normalized {:.0}% of all logical cores · write-drain p99 {:.2} ms (drain/backpressure, not QUIC CPU) · class is a heuristic not a verdict",
                cap.cpu_normalized_per_logical_pct, cap.write_drain_p99_ms
            ),
        );
    });
}

fn show_capacity_owners(ui: &mut egui::Ui, cap: &CapacityLiveSnapshot) {
    kv_row(
        ui,
        "Saturation class",
        saturation_label(cap.saturation_class),
    );
    kv_row(
        ui,
        "Dominant owner",
        cap.dominant_owner.map(TickOwnerId::as_str).unwrap_or("—"),
    );
    kv_row(
        ui,
        "Worst spike",
        cap.worst_spike_owner
            .map(TickOwnerId::as_str)
            .unwrap_or("—"),
    );
    kv_row(ui, "Unattributed mean", format_ms(cap.unattributed_mean_ms));
    if cap.accounting_error_ticks > 0 {
        kv_row(
            ui,
            "Accounting errors",
            cap.accounting_error_ticks.to_string(),
        );
    }
    ui.add_space(4.0);
    ui.colored_label(theme::muted(), "Owner share (same 120-tick window)");
    for row in &cap.owners {
        kv_row(
            ui,
            row.owner.as_str(),
            format!(
                "mean {}  p99 {}  {:.1}%",
                format_ms(row.mean_ms),
                format_ms(row.p99_ms),
                row.share_pct
            ),
        );
    }
}

fn saturation_label(class: SaturationClass) -> &'static str {
    match class {
        SaturationClass::UnknownUnattributed => "unknown/unattributed",
        SaturationClass::SimulationTick => "simulation_tick",
        SaturationClass::ServerTransportBackpressure => "server_transport_backpressure",
        SaturationClass::HarnessClient => "harness_client",
    }
}
