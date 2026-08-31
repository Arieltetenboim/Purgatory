use eframe::egui;
use purgatory_dev_runtime::{
    HubCommand, HubSnapshot, ValidationDuration, ValidationPreset, ValidationSpec, ValidationState,
};

use crate::theme;

pub fn show(
    ui: &mut egui::Ui,
    snap: &HubSnapshot,
    spec: &mut ValidationSpec,
) -> Option<HubCommand> {
    let mut cmd = None;
    ui.heading("Runtime Validation");
    ui.label("Forwards purgatory-load --preset. CLI exit is pass/fail. The Hub only orchestrates.");
    ui.add_space(8.0);

    ui.group(|ui| {
        ui.strong("Spec");
        ui.horizontal(|ui| {
            ui.label("Preset");
            egui::ComboBox::from_id_salt("rv_preset")
                .selected_text(spec.preset.as_str())
                .show_ui(ui, |ui| {
                    for preset in ValidationPreset::ALL {
                        ui.selectable_value(&mut spec.preset, preset, preset.as_str());
                    }
                });
            ui.label("Duration");
            let duration_label = spec
                .duration
                .map(ValidationDuration::as_cli)
                .unwrap_or("preset default");
            egui::ComboBox::from_id_salt("rv_duration")
                .selected_text(duration_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut spec.duration, None, "preset default");
                    for dur in ValidationDuration::ALL {
                        ui.selectable_value(&mut spec.duration, Some(dur), dur.as_cli());
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Seed");
            ui.add(egui::TextEdit::singleline(&mut spec.seed).desired_width(120.0));
        });
        ui.colored_label(
            theme::muted(),
            "START VALIDATION rebuilds purgatory-load, then restarts the server in load-mode.",
        );
    });
    ui.add_space(6.0);

    ui.group(|ui| {
        ui.strong("Run");
        ui.horizontal(|ui| {
            ui.colored_label(validation_color(snap.validation), snap.validation.as_str());
            if ui
                .add_enabled(
                    snap.can_start_validation,
                    egui::Button::new("START VALIDATION"),
                )
                .clicked()
            {
                cmd = Some(HubCommand::StartValidation { spec: spec.clone() });
            }
            if ui
                .add_enabled(
                    snap.can_stop_validation,
                    egui::Button::new("STOP VALIDATION"),
                )
                .clicked()
            {
                cmd = Some(HubCommand::StopValidation);
            }
        });
        if snap.server_state != purgatory_dev_runtime::ServerState::Ready
            && !snap.can_start_validation
        {
            ui.label("Server must be Ready before Runtime Validation.");
        }
        if let Some(reason) = &snap.validation_reason {
            ui.colored_label(theme::muted(), reason);
        }
        if let Some(line) = &snap.validation_live.status_line {
            ui.label(line);
        } else if snap.validation == ValidationState::Running {
            ui.colored_label(theme::muted(), "live_status.json not available yet");
        }
        if let Some(dir) = &snap.validation_last.dir {
            ui.label(format!("Last artifact  {}", dir.display()));
        }
        if let Some(outcome) = snap.validation_last.outcome {
            ui.label(format!("Last outcome  {}", outcome.as_str()));
        }
    });
    cmd
}

fn validation_color(state: ValidationState) -> egui::Color32 {
    match state {
        ValidationState::Passed => theme::state_color(purgatory_dev_runtime::ServerState::Ready),
        ValidationState::Failed | ValidationState::OrchestrationFailed => {
            theme::state_color(purgatory_dev_runtime::ServerState::Failed)
        }
        ValidationState::Cancelled | ValidationState::Idle => theme::muted(),
        _ => theme::state_color(purgatory_dev_runtime::ServerState::Verifying),
    }
}
