use eframe::egui;
use purgatory_dev_runtime::{
    HubCommand, HubSnapshot, ValidationDuration, ValidationPreset, ValidationSpec,
};

use crate::theme;
use crate::ui::layout::{self, PageOutcome, card};
use crate::ui::run_view::{self, RunViewState};

pub fn show(
    ui: &mut egui::Ui,
    snap: &HubSnapshot,
    spec: &mut ValidationSpec,
    run_state: &mut RunViewState,
) -> PageOutcome {
    let mut out = PageOutcome::none();
    layout::page_header(
        ui,
        "Runtime Validation",
        "Forwards purgatory-load --preset. CLI exit is pass/fail. Hub orchestrates only.",
    );

    card(ui, "Spec", |ui| {
        ui.horizontal_wrapped(|ui| {
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
            ui.label("Seed");
            ui.add(egui::TextEdit::singleline(&mut spec.seed).desired_width(100.0));
        });
        ui.colored_label(
            theme::muted(),
            "START rebuilds purgatory-load, then restarts the server in load-mode (consent).",
        );
    });
    ui.add_space(theme::SECTION_GAP);

    let model = run_view::validation_model(snap);
    let start = HubCommand::StartValidation { spec: spec.clone() };
    out.merge(run_view::show_run(
        ui,
        &model,
        run_state,
        start,
        HubCommand::StopValidation,
        |_ui, _cmd| {},
    ));
    out
}
