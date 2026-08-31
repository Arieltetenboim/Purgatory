use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot, ProcessOrigin, ServerState};

use crate::theme;
use crate::ui::status;

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    ui.heading("Dashboard");
    ui.label("Operational overview. The Hub does not own server lifetime.");
    ui.add_space(8.0);

    ui.group(|ui| {
        ui.strong("Project");
        ui.label(format!("PURGATORY  {}", snap.identity));
        ui.label(format!("PHASE  {}", snap.phase));
        ui.label(format!("Workspace  {}", snap.workspace));
        ui.label(format!("Build profile  {}", snap.build_profile));
    });
    ui.add_space(6.0);

    ui.group(|ui| {
        ui.strong("Server");
        ui.horizontal(|ui| {
            ui.colored_label(
                theme::state_color(snap.server_state),
                snap.server_state.as_str().to_uppercase(),
            );
            if snap.server_state == ServerState::Ready {
                ui.label("probe passed");
            }
        });
        ui.label(format!(
            "Origin  {}",
            snap.process_origin
                .map(ProcessOrigin::as_str)
                .unwrap_or("—")
        ));
        ui.label(format!(
            "PID  {}",
            snap.pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "—".into())
        ));
        ui.label(format!("Health  {}", snap.health.as_ui()));
        ui.label(format!("Connection  {}", snap.connection.as_ui()));
        ui.label(format!("Endpoint  {}", snap.endpoint));
    });
    ui.add_space(6.0);

    ui.group(|ui| {
        ui.strong("Active job");
        ui.label(status::job_label(snap.job));
        if snap.validation != purgatory_dev_runtime::ValidationState::Idle {
            ui.label(format!("Runtime Validation  {}", snap.validation.as_str()));
        }
    });
    ui.add_space(6.0);

    ui.group(|ui| {
        ui.strong("Quick actions");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(snap.can_start, egui::Button::new("START"))
                .clicked()
            {
                cmd = Some(HubCommand::Start);
            }
            if ui
                .add_enabled(snap.can_restart, egui::Button::new("RESTART"))
                .clicked()
            {
                cmd = Some(HubCommand::Restart);
            }
            if ui
                .add_enabled(snap.can_stop, egui::Button::new("STOP"))
                .clicked()
            {
                cmd = Some(HubCommand::Stop);
            }
        });
        if !snap.cargo_found {
            ui.colored_label(
                theme::state_color(ServerState::Degraded),
                "cargo is not on PATH",
            );
        }
    });
    ui.add_space(6.0);

    ui.group(|ui| {
        ui.strong("Recent activity");
        ui.label("Bounded in-memory view. Full log is on the Logs page and in logs/dev-tools.");
        let recent: Vec<_> = snap.log_lines.iter().rev().take(12).rev().collect();
        for line in recent {
            ui.colored_label(theme::log_color(line), line);
        }
    });
    cmd
}
