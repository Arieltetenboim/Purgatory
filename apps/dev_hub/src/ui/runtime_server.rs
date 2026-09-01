use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot, ProcessOrigin, ServerState};

use crate::theme;
use crate::ui::layout::{
    self, btn_destructive, btn_ghost, btn_primary, hub_card, log_panel_fill, metric_flow,
    status_badge,
};
use crate::ui::status;

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Runtime / Server",
        "Dedicated server control. Closing the Hub must not stop a Ready server.",
    );

    let _ = hub_card(ui, "▣", "Server", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            status_badge(
                ui,
                &snap.server_state.as_str().to_ascii_uppercase(),
                theme::state_color(snap.server_state),
            );
            if snap.server_state == ServerState::Stopped {
                ui.label(
                    egui::RichText::new("not running")
                        .font(theme::subtitle_font())
                        .color(theme::muted()),
                );
            } else if snap.server_state == ServerState::Failed {
                ui.label(
                    egui::RichText::new("failed")
                        .font(theme::subtitle_font())
                        .color(theme::destructive()),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                server_actions(ui, snap, &mut cmd);
            });
        });

        ui.add_space(4.0);

        let pid = snap
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".into());
        let origin = snap
            .process_origin
            .map(ProcessOrigin::as_str)
            .unwrap_or("—");
        let alive = if snap.process_alive { "yes" } else { "no" };
        let job = status::job_label(snap.job);
        let build = if snap.build_line.is_empty() {
            "—"
        } else {
            snap.build_line.as_str()
        };

        metric_flow(
            ui,
            &[
                ("Endpoint", snap.endpoint.as_str(), true),
                ("PID", pid.as_str(), true),
                ("Origin", origin, false),
                ("Job", job.as_str(), false),
                ("Health", snap.health.as_ui(), false),
                ("Probe", snap.connection.as_ui(), false),
                ("Listener", snap.listener.as_ui(), false),
                ("Alive", alive, false),
                ("Profile", snap.build_profile.as_str(), false),
                ("Build", build, true),
            ],
        );

        if let Some(fail) = &snap.last_failure {
            ui.add_space(2.0);
            ui.colored_label(
                theme::state_color(ServerState::Failed),
                egui::RichText::new(fail).font(theme::subtitle_font()),
            );
        }
        if !snap.cargo_found {
            ui.add_space(2.0);
            ui.colored_label(
                theme::state_color(ServerState::Degraded),
                egui::RichText::new("cargo not on PATH").font(theme::subtitle_font()),
            );
        }
    });

    ui.add_space(theme::CARD_GAP);

    let _ = hub_card(ui, "≡", "Server Log", |ui| {
        if log_panel_fill(
            ui,
            "server_log",
            &snap.server_log_lines,
            "(empty — start the server or wait for output)",
            "logs/dev-tools/server.log — scroll ↕↔",
        ) {
            cmd = Some(HubCommand::ClearServerLog);
        }
    });

    cmd
}

fn server_actions(ui: &mut egui::Ui, snap: &HubSnapshot, cmd: &mut Option<HubCommand>) {
    ui.spacing_mut().item_spacing.x = 6.0;
    match snap.server_state {
        ServerState::Stopped | ServerState::Failed => {
            if ui
                .add_enabled(snap.can_start, btn_primary("Start"))
                .clicked()
            {
                *cmd = Some(HubCommand::Start);
            }
        }
        ServerState::Ready | ServerState::Degraded => {
            if ui
                .add_enabled(snap.can_restart, btn_ghost("Restart"))
                .clicked()
            {
                *cmd = Some(HubCommand::Restart);
            }
            if ui
                .add_enabled(snap.can_stop, btn_destructive("Stop"))
                .clicked()
            {
                *cmd = Some(HubCommand::Stop);
            }
        }
        _ => {
            if ui.add_enabled(snap.can_stop, btn_ghost("Stop")).clicked() {
                *cmd = Some(HubCommand::Stop);
            }
        }
    }
}
