//! Dashboard: operational control surface for the active workspace.

use eframe::egui::{self, Align, Layout, RichText, Vec2};
use eframe::egui::text::{LayoutJob, TextFormat};

use crate::navigation::HubPage;
use crate::theme;
use crate::ui::dashboard_model::{self, AttentionVm, DashVm, ProjectVm, StatusModuleVm};
use crate::ui::layout::{self, PageOutcome, btn_destructive, btn_ghost, btn_primary, empty_state, metric_flow, status_badge};

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

    let project_w = ((avail - gap) * 0.58).floor().max(360.0);
    let actions_w = (avail - gap - project_w).floor().max(300.0);
    row_workspace_actions(ui, gap, project_w, actions_w, snap, &model, &mut outcome);
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
                    ui.label(RichText::new(icon).font(theme::state_font()).color(theme::muted()));
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

fn server_panel(
    ui: &mut egui::Ui,
    snap: &purgatory_dev_runtime::HubSnapshot,
    vm: &StatusModuleVm,
    outcome: &mut PageOutcome,
) {
    dashboard_card(ui, vm.icon, vm.title, 148.0, |ui| {
        if !vm.badge.is_empty() {
            status_badge(ui, &vm.badge, vm.badge_color);
            ui.add_space(5.0);
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
            ui.add(egui::Label::new(RichText::new(detail).font(theme::subtitle_font()).color(theme::muted())).wrap());
        }
        if !vm.metrics.is_empty() {
            ui.add_space(5.0);
            let pairs: Vec<(&str, &str, bool)> = vm.metrics.iter().map(|m| (m.key, m.value.as_str(), m.mono)).collect();
            metric_flow(ui, &pairs);
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let action = match snap.server_state {
                purgatory_dev_runtime::ServerState::Stopped | purgatory_dev_runtime::ServerState::Failed => {
                    Some(("Start Server", purgatory_dev_runtime::HubCommand::Start, snap.can_start))
                }
                purgatory_dev_runtime::ServerState::Ready | purgatory_dev_runtime::ServerState::Degraded => {
                    Some(("Restart Server", purgatory_dev_runtime::HubCommand::Restart, snap.can_restart))
                }
                _ => None,
            };
            if let Some((label, cmd, enabled)) = action
                && ui.add_enabled(enabled, btn_primary(label).min_size(Vec2::new(112.0, 30.0))).clicked()
            {
                outcome.command = Some(cmd);
            }
            if ui.add(btn_ghost("Open Server").min_size(Vec2::new(112.0, 30.0))).clicked() {
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
    dashboard_card(ui, "▣", "Clients", 148.0, |ui| {
        project_row(ui, "Running", &snap.client_count.to_string(), true);
        project_row(ui, "Queued", &snap.pending_clients.to_string(), true);
        ui.add_space(32.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui
                .add_enabled(
                    snap.can_request_clients,
                    btn_primary("+1 Client").min_size(Vec2::new(112.0, 30.0)),
                )
                .clicked()
            {
                outcome.command = Some(purgatory_dev_runtime::HubCommand::RequestClients { count: 1 });
            }
            if ui.add(btn_ghost("Open Clients").min_size(Vec2::new(112.0, 30.0))).clicked() {
                outcome.navigate = Some(HubPage::RuntimeClients);
            }
        });
    });
}

fn project_panel(ui: &mut egui::Ui, vm: &ProjectVm) {
    let (branch, commit, _dirty) = split_git_stamp(&vm.git_stamp);
    let resp = dashboard_card(ui, "▰", "Project / Workspace", 150.0, |ui| {
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
    resp.response.on_hover_text(format!("{}\n{}", vm.identity_full, vm.workspace_full));
}

fn project_grid_row(ui: &mut egui::Ui, label: &str, value: &str, mono: bool) {
    ui.label(RichText::new(label).font(theme::subtitle_font()).color(theme::muted()));
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
        ui.label(RichText::new(label).font(theme::subtitle_font()).color(theme::muted()));
        let mut value_text = RichText::new(value).color(theme::body()).strong();
        if mono {
            value_text = value_text.font(theme::mono_small());
        }
        ui.add(egui::Label::new(value_text).selectable(true));
    });
}

fn split_git_stamp(stamp: &str) -> (&str, &str, bool) {
    let (branch, commit) = stamp.split_once(" @ ").unwrap_or(("—", stamp));
    let dirty = commit.ends_with('*');
    let commit = commit.strip_suffix('*').unwrap_or(commit);
    (branch, commit, dirty)
}

fn attention_panel(ui: &mut egui::Ui, vm: &AttentionVm) {
    dashboard_card(ui, "⚑", "Attention", 148.0, |ui| {
        if vm.is_healthy() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 14.0, theme::success().gamma_multiply(0.20));
                ui.painter().circle_stroke(rect.center(), 14.0, egui::Stroke::new(1.5, theme::success()));
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "✓", theme::section_font(), theme::success());
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new("No issues requiring attention.").font(theme::section_font()).color(theme::body()).strong()).wrap());
                    ui.add(egui::Label::new(RichText::new("All systems operational.").font(theme::subtitle_font()).color(theme::muted())).wrap());
                });
            });
        } else {
            for (text, color) in &vm.issues {
                ui.horizontal(|ui| {
                    ui.colored_label(*color, "●");
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
    dashboard_card(ui, "◆", "Quick Actions", 150.0, |ui| {
        let gap = 8.0;
        let button_w = ((ui.available_width() - gap * 2.0) / 3.0).floor().max(96.0);
        let button_size = Vec2::new(button_w, 32.0);
        egui::Grid::new("dashboard_quick_actions")
            .num_columns(3)
            .spacing(Vec2::new(gap, gap))
            .show(ui, |ui| {
                if ui.add(btn_ghost("Animation Lab").min_size(button_size)).clicked() {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::LaunchAnimationLab);
                }
                if ui.add(btn_ghost("Open Logs").min_size(button_size)).clicked() {
                    outcome.navigate = Some(HubPage::Logs);
                }
                if ui.add(btn_ghost("Open Clients").min_size(button_size)).clicked() {
                    outcome.navigate = Some(HubPage::RuntimeClients);
                }
                ui.end_row();

                if ui.add(btn_ghost("Rebuild").min_size(button_size)).clicked() {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::Rebuild);
                }
                if ui.add(btn_ghost("Quality Gate").min_size(button_size)).clicked() {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::QualityGate);
                }
                if ui.add_enabled(snap.can_stop_clients, btn_ghost("Stop Clients").min_size(button_size)).clicked() {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::StopClients);
                }
                ui.end_row();

                if ui.add(btn_destructive("Kill All").min_size(button_size)).clicked() {
                    outcome.command = Some(purgatory_dev_runtime::HubCommand::KillAll);
                }
                ui.end_row();
            });

        if let Some(warn) = model.cargo_warning {
            ui.add_space(7.0);
            ui.colored_label(theme::state_color(purgatory_dev_runtime::ServerState::Degraded), warn);
        }
    });
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
                let auto_scroll_id = egui::Id::new("dashboard_activity_auto_scroll");
                let mut auto_scroll = ui.ctx().data_mut(|data| data.get_temp::<bool>(auto_scroll_id).unwrap_or(true));

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(RichText::new("≡").font(theme::state_font()).color(theme::muted()));
                    ui.label(RichText::new("Recent Activity").font(theme::state_font()).color(theme::body()).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(btn_ghost("Clear")).clicked() {
                            outcome.command = Some(purgatory_dev_runtime::HubCommand::ClearActivityLog);
                        }
                        if ui.add(btn_ghost("Open Logs")).clicked() {
                            outcome.navigate = Some(HubPage::Logs);
                        }
                        if ui.checkbox(&mut auto_scroll, "Auto-scroll").changed() {
                            ui.ctx().data_mut(|data| data.insert_temp(auto_scroll_id, auto_scroll));
                        }
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(7.0);

                let full_width = ui.available_width();
                egui::Resize::default()
                    .id_salt("dashboard_activity_resize")
                    .default_size(Vec2::new(full_width, 270.0))
                    .min_width(full_width)
                    .max_width(full_width)
                    .min_height(160.0)
                    .max_height(430.0)
                    .show(ui, |ui| {
                        ui.set_min_width(full_width);
                        ui.set_max_width(full_width);
                        egui::Frame::new()
                            .fill(theme::bg())
                            .stroke(theme::card_stroke())
                            .corner_radius(5.0)
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                if snap.log_lines.is_empty() {
                                    empty_state(ui, "No activity yet.");
                                    return;
                                }
                                let display_text = decorate_activity(&snap.log_lines);
                                let mut readonly = display_text.as_str();
                                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                                    let mut job = activity_layout_job(text.as_str());
                                    job.wrap.max_width = wrap_width;
                                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                                };
                                egui::ScrollArea::vertical()
                                    .id_salt("dashboard_activity_scroll")
                                    .auto_shrink([false, false])
                                    .stick_to_bottom(auto_scroll)
                                    .show(ui, |ui| {
                                        let width = ui.available_width();
                                        let height = ui.available_height().max(140.0);
                                        ui.add_sized(
                                            [width, height],
                                            egui::TextEdit::multiline(&mut readonly)
                                                .font(theme::mono_small())
                                                .desired_width(f32::INFINITY)
                                                .layouter(&mut layouter),
                                        );
                                    });
                            });
                    });
            });
    });
}

fn decorate_activity(lines: &[String]) -> String {
    lines.iter().map(|line| {
        let (stamp, message) = split_timestamp(line);
        let kind = activity_kind(message);
        if stamp.is_empty() { format!("[{kind}] {message}") } else { format!("{stamp}  [{kind}] {message}") }
    }).collect::<Vec<_>>().join("\n")
}

fn split_timestamp(line: &str) -> (&str, &str) {
    if line.len() >= 8 {
        let bytes = line.as_bytes();
        if bytes.get(2) == Some(&b':') && bytes.get(5) == Some(&b':') {
            return (&line[..8], line[8..].trim_start());
        }
    }
    ("", line)
}

fn activity_kind(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("fail") || lower.contains("error") || lower.contains("panic") {
        "ERROR"
    } else if lower.contains("warn") || lower.contains("degraded") || lower.contains("not running") {
        "WARN"
    } else if lower.contains("build") || lower.contains("rebuild") || lower.contains("cargo") || lower.contains("compil") {
        "BUILD"
    } else if lower.contains("client") {
        "CLIENT"
    } else if lower.contains("server") || lower.contains("probe") || lower.contains("listener") {
        "SERVER"
    } else if lower.contains("load") {
        "LOAD"
    } else {
        "INFO"
    }
}

fn activity_layout_job(text: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = theme::mono_small();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            job.append("\n", 0.0, TextFormat { font_id: font.clone(), color: theme::body(), ..Default::default() });
        }
        let (stamp, rest) = split_timestamp(line);
        let rest = rest.trim_start();
        if !stamp.is_empty() {
            job.append(stamp, 0.0, TextFormat { font_id: font.clone(), color: theme::muted(), ..Default::default() });
            job.append("  ", 0.0, TextFormat { font_id: font.clone(), color: theme::muted(), ..Default::default() });
        }
        if let Some(end) = rest.find(']')
            && rest.starts_with('[')
        {
            let tag = &rest[..=end];
            let message = rest[end + 1..].trim_start();
            job.append(tag, 0.0, TextFormat { font_id: font.clone(), color: activity_tag_color(tag), ..Default::default() });
            job.append("  ", 0.0, TextFormat { font_id: font.clone(), color: theme::body(), ..Default::default() });
            job.append(message, 0.0, TextFormat { font_id: font.clone(), color: theme::body(), ..Default::default() });
        } else {
            job.append(rest, 0.0, TextFormat { font_id: font.clone(), color: theme::body(), ..Default::default() });
        }
    }
    job
}

fn activity_tag_color(tag: &str) -> egui::Color32 {
    match tag {
        "[ERROR]" => theme::destructive(),
        "[WARN]" => egui::Color32::from_rgb(220, 160, 50),
        "[BUILD]" => egui::Color32::from_rgb(190, 110, 220),
        "[CLIENT]" => egui::Color32::from_rgb(80, 180, 210),
        "[SERVER]" => theme::success(),
        "[LOAD]" => egui::Color32::from_rgb(170, 140, 210),
        _ => theme::accent(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_git_stamp_reports_branch_commit_and_dirty() {
        assert_eq!(split_git_stamp("hub-0.4 @ 218615b*"), ("hub-0.4", "218615b", true));
        assert_eq!(split_git_stamp("master @ abc1234"), ("master", "abc1234", false));
    }

    #[test]
    fn activity_kind_is_stable_for_common_hub_lines() {
        assert_eq!(activity_kind("Building purgatory-client"), "BUILD");
        assert_eq!(activity_kind("Opening client 2"), "CLIENT");
        assert_eq!(activity_kind("Server ready"), "SERVER");
        assert_eq!(activity_kind("Server is not running"), "WARN");
        assert_eq!(activity_kind("build failed"), "ERROR");
    }
}
