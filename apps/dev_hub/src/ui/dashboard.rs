//! Dashboard: operational control surface for the active workspace.

use eframe::egui::{self, Align, Layout, RichText, Vec2};

use crate::navigation::HubPage;
use crate::theme;
use crate::ui::dashboard_model::{
    self, ActionKind, AttentionVm, DashVm, ProjectVm, StatusModuleVm,
};
use crate::ui::layout::{
    self, PageOutcome, btn_destructive, btn_ghost, btn_primary, empty_state, hub_card,
    metric_flow, metric_row, status_badge,
};

pub fn show(ui: &mut egui::Ui, snap: &purgatory_dev_runtime::HubSnapshot) -> PageOutcome {
    let mut outcome = PageOutcome::none();
    let model = dashboard_model::from_snapshot(snap);

    layout::page_header(
        ui,
        "Dashboard",
        "Control your development environment and monitor runtime status.",
    );

    let gap = theme::CARD_GAP;
    let avail = ui.available_width();
    let col = ((avail - gap * 2.0) / 3.0).floor().max(200.0);

    row_top(ui, gap, col, snap, &model, &mut outcome);
    ui.add_space(gap);

    let project_w = ((avail - gap) * 0.58).floor().max(340.0);
    let actions_w = (avail - gap - project_w).floor().max(280.0);
    row_workspace_actions(
        ui,
        gap,
        project_w,
        actions_w,
        snap,
        &model,
        &mut outcome,
    );
    ui.add_space(gap);

    activity_panel(ui, snap, &mut outcome);

    outcome
}

fn row_top(
    ui: &mut egui::Ui,
    gap: f32,
    col: f32,
    snap: &purgatory_dev_runtime::HubSnapshot,
    model: &DashVm,
    outcome: &mut PageOutcome,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 0.0),
        Layout::left_to_right(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
            cell(ui, col, |ui| status_card(ui, &model.server, outcome));
            cell(ui, col, |ui| clients_panel(ui, snap, outcome));
            cell(ui, col, |ui| attention_panel(ui, &model.attention));
        },
    );
}

fn row_workspace_actions(
    ui: &mut egui::Ui,
    gap: f32,
    project_w: f32,
    actions_w: f32,
    snap: &purgatory_dev_runtime::HubSnapshot,
    model: &DashVm,
    outcome: &mut PageOutcome,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), 0.0),
        Layout::left_to_right(Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
            cell(ui, project_w, |ui| project_panel(ui, &model.project));
            cell(ui, actions_w, |ui| quick_actions(ui, snap, model, outcome));
        },
    );
}

fn cell(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        add(ui);
    });
}

fn full_width(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    cell(ui, w, add);
}

fn status_card(ui: &mut egui::Ui, vm: &StatusModuleVm, outcome: &mut PageOutcome) {
    hub_card(ui, vm.icon, vm.title, |ui| {
        if !vm.badge.is_empty() {
            status_badge(ui, &vm.badge, vm.badge_color);
            ui.add_space(4.0);
        }
        ui.add(
            egui::Label::new(
                RichText::new(&vm.headline)
                    .font(theme::state_font())
                    .color(theme::body())
                    .strong(),
            )
            .wrap(),
        );
        if let Some(detail) = &vm.detail {
            ui.add(
                egui::Label::new(
                    RichText::new(detail)
                        .font(theme::subtitle_font())
                        .color(theme::muted()),
                )
                .wrap(),
            );
        }
        if !vm.metrics.is_empty() {
            ui.add_space(4.0);
            let pairs: Vec<(&str, &str, bool)> = vm
                .metrics
                .iter()
                .map(|m| (m.key, m.value.as_str(), m.mono))
                .collect();
            metric_flow(ui, &pairs);
        }
        if !vm.detail_metrics.is_empty() {
            egui::CollapsingHeader::new("Details")
                .default_open(false)
                .show(ui, |ui| {
                    let pairs: Vec<(&str, &str, bool)> = vm
                        .detail_metrics
                        .iter()
                        .map(|m| (m.key, m.value.as_str(), m.mono))
                        .collect();
                    metric_flow(ui, &pairs);
                });
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some((label, cmd)) = &vm.primary_action
                && ui.add(btn_primary(label)).clicked()
            {
                outcome.command = Some(cmd.clone());
            }
            for (action, enabled) in &vm.secondary_actions {
                if ui
                    .add_enabled(*enabled, btn_ghost(action.label()))
                    .clicked()
                {
                    outcome.command = Some(action.command());
                }
            }
            if let Some(nav) = vm.nav_label
                && ui.add(btn_ghost(nav)).clicked()
            {
                outcome.navigate = Some(nav_page(nav));
            }
        });
    });
}

fn nav_page(label: &str) -> HubPage {
    match label {
        "Open Server" => HubPage::RuntimeServer,
        "Open Clients" => HubPage::RuntimeClients,
        "Open Performance" => HubPage::Performance,
        "Open Logs" => HubPage::Logs,
        _ => HubPage::Validation,
    }
}

fn clients_panel(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    outcome: &mut PageOutcome,
) {
    hub_card(ui, "▣", "Clients", |ui| {
        metric_row(ui, "Running", &snap.client_count.to_string(), true);
        metric_row(ui, "Queued", &snap.pending_clients.to_string(), true);
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for count in [1_u32, 2, 3] {
                if ui
                    .add_enabled(
                        snap.can_request_clients,
                        btn_primary(&format!("+ {count}")),
                    )
                    .clicked()
                {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::RequestClients {
                        count,
                    });
                }
            }
            if ui
                .add_enabled(snap.can_stop_clients, btn_ghost("Stop All"))
                .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::StopClients);
            }
        });
        ui.add_space(4.0);
        if ui.add(btn_ghost("Open Clients")).clicked() {
            outcome.navigate = Some(HubPage::RuntimeClients);
        }
    });
}

fn project_panel(ui: &mut egui::Ui, vm: &ProjectVm) {
    let (branch, commit, dirty) = split_git_stamp(&vm.build);
    let resp = hub_card(ui, "▤", "Project / Workspace", |ui| {
        metric_row(ui, "Workspace", vm.workspace_short.as_str(), true);
        metric_row(ui, "Branch", branch, true);
        metric_row(ui, "Commit", commit, true);
        metric_row(
            ui,
            "Working tree",
            if dirty { "DIRTY" } else { "CLEAN" },
            false,
        );
    });
    resp.response
        .on_hover_text(format!("{}\n{}", vm.identity_full, vm.workspace_full));
}

fn split_git_stamp(stamp: &str) -> (&str, &str, bool) {
    let (branch, commit) = stamp.split_once(" @ ").unwrap_or(("—", stamp));
    let dirty = commit.ends_with('*');
    let commit = commit.strip_suffix('*').unwrap_or(commit);
    (branch, commit, dirty)
}

fn attention_panel(ui: &mut egui::Ui, vm: &AttentionVm) {
    hub_card(ui, "⚑", "Attention", |ui| {
        if vm.is_healthy() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    12.0,
                    theme::success().gamma_multiply(0.22),
                );
                ui.painter().circle_stroke(
                    rect.center(),
                    12.0,
                    egui::Stroke::new(1.5, theme::success()),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "✓",
                    theme::section_font(),
                    theme::success(),
                );
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new("No issues requiring attention.")
                                .color(theme::body())
                                .strong(),
                        )
                        .wrap(),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new("All systems operational.")
                                .font(theme::subtitle_font())
                                .color(theme::muted()),
                        )
                        .wrap(),
                    );
                });
            });
        } else {
            for (text, color) in &vm.issues {
                ui.horizontal(|ui| {
                    ui.colored_label(*color, "●");
                    ui.add(egui::Label::new(RichText::new(text).color(*color)).wrap());
                });
                ui.add_space(2.0);
            }
        }
    });
}

fn quick_actions(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    model: &DashVm,
    outcome: &mut PageOutcome,
) {
    hub_card(ui, "☰", "Quick Actions", |ui| {
        ui.label(
            RichText::new("Tools")
                .font(theme::subtitle_font())
                .color(theme::muted())
                .strong(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for action in &model.actions {
                if action.label != "Animation Lab" {
                    continue;
                }
                if ui
                    .add_enabled(action.enabled, btn_ghost(action.label))
                    .clicked()
                {
                    outcome.command = Some(action.command.clone());
                }
            }
            if ui.add(btn_ghost("Open Logs")).clicked() {
                outcome.navigate = Some(HubPage::Logs);
            }
        });

        ui.add_space(8.0);
        ui.label(
            RichText::new("Operations")
                .font(theme::subtitle_font())
                .color(theme::muted())
                .strong(),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for action in &model.actions {
                if matches!(action.label, "Animation Lab" | "+ 1 Client") {
                    continue;
                }
                let clicked = match action.kind {
                    ActionKind::Routine => ui
                        .add_enabled(action.enabled, btn_primary(action.label))
                        .clicked(),
                    ActionKind::Secondary => ui
                        .add_enabled(action.enabled, btn_ghost(action.label))
                        .clicked(),
                    ActionKind::Destructive => ui
                        .add_enabled(action.enabled, btn_destructive(action.label))
                        .clicked(),
                };
                if clicked {
                    outcome.command = Some(action.command.clone());
                }
            }
            if ui
                .add_enabled(
                    !snap.phase78_gate_active,
                    btn_ghost("Phase 7.8 Gate"),
                )
                .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::Phase78Gate);
            }
        });

        if let Some(warn) = model.cargo_warning {
            ui.add_space(4.0);
            ui.colored_label(
                theme::state_color(purgatory_dev_runtime::ServerState::Degraded),
                warn,
            );
        }
    });
}

fn activity_panel(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    outcome: &mut PageOutcome,
) {
    full_width(ui, |ui| {
        hub_card(ui, "≡", "Recent Activity", |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(btn_ghost("Clear")).clicked() {
                        outcome.command = Some(purgatory_dev_runtime::HubCommand::ClearActivityLog);
                    }
                    if ui.add(btn_ghost("Open Logs")).clicked() {
                        outcome.navigate = Some(HubPage::Logs);
                    }
                });
            });
            ui.add_space(4.0);

            egui::Resize::default()
                .id_salt("dash_activity_resize")
                .default_height(250.0)
                .min_height(130.0)
                .max_height(430.0)
                .show(ui, |ui| {
                    if snap.log_lines.is_empty() {
                        empty_state(ui, "No activity yet.");
                        return;
                    }
                    let mut text = snap.log_lines.join("\n");
                    egui::ScrollArea::both()
                        .id_salt("dash_activity_scroll")
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add_sized(
                                [ui.available_width(), ui.available_height().max(120.0)],
                                egui::TextEdit::multiline(&mut text)
                                    .font(theme::mono_small())
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::split_git_stamp;

    #[test]
    fn git_stamp_separates_branch_commit_and_dirty_state() {
        assert_eq!(
            split_git_stamp("hub-0.4 @ abc1234*"),
            ("hub-0.4", "abc1234", true)
        );
        assert_eq!(
            split_git_stamp("master @ abc1234"),
            ("master", "abc1234", false)
        );
    }
}
