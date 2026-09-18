//! Dashboard: operational control surface for the active workspace.

use std::path::PathBuf;

use eframe::egui::{self, Align, Layout, RichText, Vec2};

use crate::navigation::HubPage;
use crate::theme;
use crate::ui::dashboard_model::{self, AttentionVm, DashVm, ProjectVm, StatusModuleVm};
use crate::ui::layout::{
    self, PageOutcome, btn_destructive, btn_ghost, btn_primary, metric_flow, status_badge,
};
use crate::ui::{gate_pipeline, log_console, tool_launch};

pub fn show(ui: &mut egui::Ui, snap: &purgatory_dev_runtime::HubSnapshot) -> PageOutcome {
    let mut outcome = PageOutcome::none();
    let mut model = dashboard_model::from_snapshot(snap);
    let gate_log = PathBuf::from(&snap.log_dir).join("quality-gate.log");
    if let Some(step) = gate_pipeline::failed_step(&gate_log) {
        model.attention.issues.push((
            format!("Quality Gate failed at {step}"),
            theme::destructive(),
        ));
    }

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

    let project_w = ((avail - gap) * 0.58).floor().max(360.0);
    let actions_w = (avail - gap - project_w).floor().max(300.0);
    row_workspace_actions(ui, gap, project_w, actions_w, snap, &model, &mut outcome);
    ui.add_space(gap);

    gate_pipeline::show(ui, &gate_log);
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
            cell(ui, col, |ui| server_panel(ui, snap, &model.server, outcome));
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

fn dashboard_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    min_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<()> {
    egui::Frame::new()
        .fill(theme::card_fill_elevated())
        .stroke(theme::card_stroke())
        .corner_radius(theme::CARD_RADIUS)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(min_height);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if !icon.is_empty() {
                    ui.label(
                        RichText::new(icon)
                            .font(theme::state_font())
                            .color(theme::muted()),
                    );
                }
                ui.label(
                    RichText::new(title)
                        .font(theme::state_font())
                        .color(theme::body())
                        .strong(),
                );
            });
            ui.add_space(9.0);
            add_contents(ui);
        })
}

fn action_response(response: egui::Response, tooltip: &str) -> egui::Response {
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

fn server_panel(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    vm: &StatusModuleVm,
    outcome: &mut PageOutcome,
) {
    dashboard_card(ui, vm.icon, vm.title, 148.0, |ui| {
        status_badge(ui, &vm.badge, vm.badge_color);
        ui.add_space(5.0);
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
            ui.add_space(5.0);
            let pairs: Vec<(&str, &str, bool)> = vm
                .metrics
                .iter()
                .map(|m| (m.key, m.value.as_str(), m.mono))
                .collect();
            metric_flow(ui, &pairs);
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            match snap.server_state {
                purgatory_dev_runtime::ServerState::Stopped
                | purgatory_dev_runtime::ServerState::Failed => {
                    let response = ui.add_enabled(
                        snap.can_start,
                        btn_primary("Start Server").min_size(Vec2::new(112.0, 30.0)),
                    );
                    if action_response(
                        response,
                        "Build and start the dedicated server, then verify readiness before marking it Ready.",
                    )
                    .clicked()
                    {
                        outcome.command = Some(purgatory_dev_runtime::HubCommand::Start);
                    }
                }
                purgatory_dev_runtime::ServerState::Ready
                | purgatory_dev_runtime::ServerState::Degraded => {
                    let response = ui.add_enabled(
                        snap.can_restart,
                        btn_primary("Restart Server").min_size(Vec2::new(112.0, 30.0)),
                    );
                    if action_response(
                        response,
                        "Stop the current dedicated server and start it again through the normal readiness checks.",
                    )
                    .clicked()
                    {
                        outcome.command = Some(purgatory_dev_runtime::HubCommand::Restart);
                    }

                    let response = ui.add_enabled(
                        snap.can_stop,
                        btn_destructive("Stop").min_size(Vec2::new(72.0, 30.0)),
                    );
                    if action_response(
                        response,
                        "Stop the dedicated server. Detached clients are not stopped.",
                    )
                    .clicked()
                    {
                        outcome.command = Some(purgatory_dev_runtime::HubCommand::Stop);
                    }
                }
                _ => {}
            }
            let response = ui.add(btn_ghost("Open Server").min_size(Vec2::new(112.0, 30.0)));
            if action_response(
                response,
                "Open server lifecycle controls, diagnostics, health state, and the dedicated server log.",
            )
            .clicked()
            {
                outcome.navigate = Some(HubPage::RuntimeServer);
            }
        });
    });
}

fn clients_panel(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    outcome: &mut PageOutcome,
) {
    dashboard_card(ui, "C", "Clients", 148.0, |ui| {
        project_row(ui, "Running", &snap.client_count.to_string(), true);
        project_row(ui, "Queued", &snap.pending_clients.to_string(), true);
        ui.add_space(32.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let response = ui.add_enabled(
                snap.can_request_clients,
                btn_primary("+1 Client").min_size(Vec2::new(112.0, 30.0)),
            );
            if action_response(
                response,
                "Queue one game client. It launches once the server is Ready; the client is rebuilt first when possible.",
            )
            .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::RequestClients { count: 1 });
            }

            let response = ui.add(btn_ghost("Open Clients").min_size(Vec2::new(112.0, 30.0)));
            if action_response(
                response,
                "Open client controls, running/queued counts, and the client log.",
            )
            .clicked()
            {
                outcome.navigate = Some(HubPage::RuntimeClients);
            }
        });
    });
}

fn project_panel(ui: &mut egui::Ui, vm: &ProjectVm) {
    let (branch, commit, _dirty) = split_git_stamp(&vm.git_stamp);
    let resp = dashboard_card(ui, "P", "Project / Workspace", 150.0, |ui| {
        ui.separator();
        ui.add_space(5.0);
        egui::Grid::new("dashboard_project_grid")
            .num_columns(2)
            .spacing(Vec2::new(18.0, 8.0))
            .show(ui, |ui| {
                project_grid_row(ui, "Workspace", vm.workspace_short.as_str(), true);
                project_grid_row(ui, "Branch", branch, true);
                project_grid_row(ui, "Commit", commit, true);
            });
    });
    resp.response
        .on_hover_text(format!("{}\n{}", vm.identity_full, vm.workspace_full));
}

fn project_grid_row(ui: &mut egui::Ui, label: &str, value: &str, mono: bool) {
    ui.label(
        RichText::new(label)
            .font(theme::subtitle_font())
            .color(theme::muted()),
    );
    let mut value_text = RichText::new(value).color(theme::body()).strong();
    if mono {
        value_text = value_text.font(theme::mono_small());
    }
    ui.add(egui::Label::new(value_text).selectable(true));
    ui.end_row();
}

fn project_row(ui: &mut egui::Ui, label: &str, value: &str, mono: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            RichText::new(label)
                .font(theme::subtitle_font())
                .color(theme::muted()),
        );
        let mut value_text = RichText::new(value).color(theme::body()).strong();
        if mono {
            value_text = value_text.font(theme::mono_small());
        }
        ui.add(egui::Label::new(value_text).selectable(true));
    });
}

fn split_git_stamp(stamp: &str) -> (&str, &str, bool) {
    let (branch, commit) = stamp.split_once(" @ ").unwrap_or(("-", stamp));
    let dirty = commit.ends_with('*');
    let commit = commit.strip_suffix('*').unwrap_or(commit);
    (branch, commit, dirty)
}

fn attention_panel(ui: &mut egui::Ui, vm: &AttentionVm) {
    dashboard_card(ui, "!", "Attention", 148.0, |ui| {
        if vm.is_healthy() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    14.0,
                    theme::success().gamma_multiply(0.20),
                );
                ui.painter().circle_stroke(
                    rect.center(),
                    14.0,
                    egui::Stroke::new(1.5, theme::success()),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "OK",
                    theme::subtitle_font(),
                    theme::success(),
                );
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new("No issues requiring attention.")
                                .font(theme::section_font())
                                .color(theme::body())
                                .strong(),
                        )
                        .wrap(),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new("Current local checks are operational.")
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
                    ui.colored_label(*color, "!");
                    ui.add(egui::Label::new(RichText::new(text).color(*color)).wrap());
                });
                ui.add_space(3.0);
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
    dashboard_card(ui, "Q", "Quick Actions", 150.0, |ui| {
        ui.colored_label(
            theme::muted(),
            RichText::new("Hover an action for details.").font(theme::subtitle_font()),
        );
        ui.add_space(6.0);

        let gap = 8.0;
        let button_w = ((ui.available_width() - gap * 2.0) / 3.0).floor().max(96.0);
        let button_size = Vec2::new(button_w, 32.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let response = ui.add(btn_ghost("Animation Lab").min_size(button_size));
            if action_response(
                response,
                "Launch the standalone animation authoring tool. It runs independently of the game server and clients.",
            )
            .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::LaunchAnimationLab);
            }
            let response = ui.add(btn_ghost("NPC Lab").min_size(button_size));
            if action_response(
                response,
                "Launch the local NPC authoring web tool in the background. Output is written to logs/dev-tools/npc-lab.log.",
            )
            .clicked()
            {
                let _ = tool_launch::launch_npc_lab();
            }
            let response = ui.add(btn_ghost("Hub Logs").min_size(button_size));
            if action_response(
                response,
                "Open the Logs page, including Hub activity and isolated Quality Gate output.",
            )
            .clicked()
            {
                outcome.navigate = Some(HubPage::Logs);
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let response = rebuild_button(ui, button_size);
            if action_response(
                response,
                "Rebuild available Hub binaries. Running or locked executables are skipped rather than forcibly stopped.",
            )
            .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::Rebuild);
            }
            let response = ui.add(btn_ghost("Quality Gate").min_size(button_size));
            if action_response(
                response,
                "Run Format -> Cargo Check -> Clippy -> Workspace Tests -> Content Validation. Progress appears below; detailed output stays in Logs -> Quality Gate.",
            )
            .clicked()
            {
                let _ = tool_launch::launch_quality_gate();
            }
            let response = ui.add_enabled(
                snap.can_stop_clients,
                btn_ghost("Stop Clients").min_size(button_size),
            );
            if action_response(
                response,
                "Stop all workspace game clients and clear queued client launches.",
            )
            .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::StopClients);
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        let response = ui.add(btn_destructive("Kill All (F9)").min_size(button_size));
        if action_response(
            response,
            "Emergency cleanup for game runtime processes: stop server, clients, load/validation jobs, active builds, and workspace Cargo processes. Authoring tools remain independent.",
        )
        .clicked()
        {
            outcome.command = Some(purgatory_dev_runtime::HubCommand::KillAll);
        }

        if let Some(warn) = model.cargo_warning {
            ui.add_space(7.0);
            ui.colored_label(
                theme::state_color(purgatory_dev_runtime::ServerState::Degraded),
                warn,
            );
        }
    });
}

fn rebuild_button(ui: &mut egui::Ui, size: Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if response.hovered() {
        egui::Color32::from_rgb(34, 40, 48)
    } else {
        egui::Color32::from_rgb(28, 34, 42)
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter()
        .rect_stroke(rect, 6.0, theme::card_stroke(), egui::StrokeKind::Inside);

    let icon_center = egui::pos2(rect.left() + 18.0, rect.center().y);
    let stroke = egui::Stroke::new(1.5, theme::body());
    ui.painter().line_segment(
        [
            egui::pos2(icon_center.x - 3.0, icon_center.y + 4.0),
            egui::pos2(icon_center.x + 3.0, icon_center.y - 4.0),
        ],
        stroke,
    );
    ui.painter().rect_stroke(
        egui::Rect::from_center_size(
            egui::pos2(icon_center.x + 4.0, icon_center.y - 5.0),
            Vec2::new(7.0, 4.0),
        ),
        1.0,
        stroke,
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(icon_center.x + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Rebuild",
        theme::section_font(),
        theme::body(),
    );
    response
}

fn activity_panel(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    outcome: &mut PageOutcome,
) {
    full_width(ui, |ui| {
        egui::Frame::new()
            .fill(theme::card_fill_elevated())
            .stroke(theme::card_stroke())
            .corner_radius(theme::CARD_RADIUS)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(
                        RichText::new("LOG")
                            .font(theme::subtitle_font())
                            .color(theme::muted()),
                    );
                    ui.label(
                        RichText::new("Recent Activity")
                            .font(theme::state_font())
                            .color(theme::body())
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let response = ui.add(btn_ghost("Open Logs"));
                        if action_response(
                            response,
                            "Open the full Logs page, including isolated Quality Gate output.",
                        )
                        .clicked()
                        {
                            outcome.navigate = Some(HubPage::Logs);
                        }
                    });
                });
                ui.add_space(7.0);
                if log_console::show(
                    ui,
                    "dashboard_activity",
                    &snap.log_lines,
                    "INFO",
                    "No activity yet.",
                ) {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::ClearActivityLog);
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_git_stamp_reports_branch_commit_and_dirty() {
        assert_eq!(
            split_git_stamp("hub-0.4 @ 218615b*"),
            ("hub-0.4", "218615b", true)
        );
        assert_eq!(
            split_git_stamp("master @ abc1234"),
            ("master", "abc1234", false)
        );
    }
}
