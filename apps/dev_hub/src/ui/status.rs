use eframe::egui;
use purgatory_dev_runtime::JobPhase;

use crate::theme;

#[must_use]
pub fn job_label(job: JobPhase) -> String {
    match job {
        JobPhase::Idle => "None".to_string(),
        JobPhase::Running { op, id } => format!("{} #{id}", op.as_str()),
        JobPhase::Cancelling { op, id } => format!("cancelling {} #{id}", op.as_str()),
    }
}

pub fn bar(ui: &mut egui::Ui, snap: &purgatory_dev_runtime::HubSnapshot) {
    ui.horizontal(|ui| {
        ui.colored_label(
            theme::state_color(snap.server_state),
            snap.server_state.as_str().to_uppercase(),
        );
        ui.separator();
        ui.label(format!("job {}", job_label(snap.job)));
        if snap.validation != purgatory_dev_runtime::ValidationState::Idle {
            ui.separator();
            ui.label(format!("rv {}", snap.validation.as_str()));
        }
        if let Some(pid) = snap.pid {
            ui.separator();
            ui.label(format!("pid {pid}"));
        }
        if let Some(fail) = &snap.last_failure {
            ui.separator();
            ui.colored_label(
                theme::state_color(purgatory_dev_runtime::ServerState::Failed),
                fail,
            );
        }
    });
}
