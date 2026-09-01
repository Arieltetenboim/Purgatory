use eframe::egui::{self, Sense, Vec2};
use purgatory_dev_runtime::{JobPhase, ServerState};

use crate::theme;

#[must_use]
pub fn job_label(job: JobPhase) -> String {
    match job {
        JobPhase::Idle => "None".to_string(),
        JobPhase::Running { op, id } => format!("{} #{id}", op.as_str()),
        JobPhase::Cancelling { op, id } => format!("cancelling {} #{id}", op.as_str()),
    }
}

/// Footer status bar: live job/clients context + shortcuts.
pub fn bar(ui: &mut egui::Ui, snap: &purgatory_dev_runtime::HubSnapshot) {
    ui.horizontal(|ui| {
        if snap.validation != purgatory_dev_runtime::ValidationState::Idle {
            ui.colored_label(theme::muted(), format!("rv {}", snap.validation.as_str()));
            ui.separator();
        }
        if snap.load != purgatory_dev_runtime::LoadState::Idle {
            ui.colored_label(theme::muted(), format!("load {}", snap.load.as_str()));
            ui.separator();
        }
        if snap.client_count > 0 || snap.pending_clients > 0 {
            ui.colored_label(
                theme::muted(),
                format!("clients {}/+{}", snap.client_count, snap.pending_clients),
            );
            ui.separator();
        }
        if let Some(pid) = snap.pid {
            ui.colored_label(theme::muted(), format!("pid {pid}"));
        }
        if let Some(fail) = &snap.last_failure {
            ui.separator();
            ui.colored_label(theme::destructive(), fail);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(
                theme::muted(),
                egui::RichText::new("F5 start/restart    F6 client").font(theme::subtitle_font()),
            );
        });
    });
}

/// Sidebar bottom strip from real server/job state (not a fake account block).
pub fn connection_strip(ui: &mut egui::Ui, snap: &purgatory_dev_runtime::HubSnapshot) {
    let (label, color) = connection_label(snap.server_state);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 3.5, color);
        ui.label(
            egui::RichText::new(label)
                .strong()
                .color(color)
                .font(theme::section_font()),
        );
    });
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(format!("Job {}", job_label(snap.job)))
            .font(theme::subtitle_font())
            .color(theme::muted()),
    );
}

#[must_use]
pub fn connection_label(state: ServerState) -> (&'static str, egui::Color32) {
    use ServerState::*;
    match state {
        Ready => ("CONNECTED", theme::success()),
        Degraded => ("DEGRADED", theme::state_color(Degraded)),
        Failed => ("FAILED", theme::destructive()),
        Stopped => ("STOPPED", theme::muted()),
        Building => ("BUILDING", theme::state_color(Building)),
        Starting => ("STARTING", theme::state_color(Starting)),
        Verifying => ("VERIFYING", theme::state_color(Verifying)),
        Stopping => ("STOPPING", theme::state_color(Stopping)),
    }
}
