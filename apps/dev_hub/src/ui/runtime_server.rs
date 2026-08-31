use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot, ProcessOrigin, ServerState};

use crate::theme;

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    ui.heading("Runtime / Server");
    ui.label("Dedicated server control. Closing the Hub must not stop a Ready server.");
    ui.add_space(8.0);

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
            "cargo is not on PATH — start/rebuild disabled",
        );
    }
    ui.add_space(8.0);

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong("State");
            ui.colored_label(
                theme::state_color(snap.server_state),
                snap.server_state.as_str().to_uppercase(),
            );
        });
        ui.label(format!("Build  {}", snap.build_line));
        ui.label(format!(
            "Origin  {}",
            snap.process_origin
                .map(ProcessOrigin::as_str)
                .unwrap_or("none")
        ));
        ui.label(format!(
            "PID  {}",
            snap.pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "—".into())
        ));
        ui.label(format!(
            "Process alive  {}",
            if snap.process_alive { "yes" } else { "no" }
        ));
        ui.label(format!("Health  {}", snap.health.as_ui()));
        ui.label(format!("Connection / probe  {}", snap.connection.as_ui()));
        ui.label(format!("Listener (diag)  {}", snap.listener.as_ui()));
        ui.label(format!("Endpoint  {}", snap.endpoint));
        ui.label(format!("Job  {}", crate::ui::status::job_label(snap.job)));
        if let Some(fail) = &snap.last_failure {
            ui.colored_label(theme::state_color(ServerState::Failed), fail);
        }
    });
    cmd
}
