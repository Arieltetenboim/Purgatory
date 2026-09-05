use eframe::egui;
use purgatory_dev_runtime::{
    HubCommand, HubSnapshot, LoadDuration, LoadProfile, LoadScenario, LoadSpec,
};

use crate::theme;
use crate::ui::layout::{self, PageOutcome, card};
use crate::ui::run_view::{self, RunViewState};

pub fn show(
    ui: &mut egui::Ui,
    snap: &HubSnapshot,
    spec: &mut LoadSpec,
    run_state: &mut RunViewState,
) -> PageOutcome {
    let mut out = PageOutcome::none();
    layout::page_header(
        ui,
        "Performance / Load",
        "Headless bots via purgatory-load. May restart the server in load-mode. Canonical Phase 7.8 regression gate: Settings → PHASE 7.8 GATE. Artifact summary: Testing → Phase 7 Stats.",
    );

    card(ui, "Spec", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Count");
            egui::ComboBox::from_id_salt("load_count")
                .selected_text(spec.count.to_string())
                .show_ui(ui, |ui| {
                    for c in LoadSpec::COUNTS {
                        ui.selectable_value(&mut spec.count, c, c.to_string());
                    }
                });
            ui.label("Profile");
            egui::ComboBox::from_id_salt("load_profile")
                .selected_text(spec.profile.as_str())
                .show_ui(ui, |ui| {
                    for p in LoadProfile::ALL {
                        ui.selectable_value(&mut spec.profile, p, p.as_str());
                    }
                });
            ui.label("Scenario");
            egui::ComboBox::from_id_salt("load_scenario")
                .selected_text(spec.scenario.as_str())
                .show_ui(ui, |ui| {
                    for s in LoadScenario::ALL {
                        ui.selectable_value(&mut spec.scenario, s, s.as_str());
                    }
                });
            ui.label("Duration");
            egui::ComboBox::from_id_salt("load_duration")
                .selected_text(spec.duration.as_cli())
                .show_ui(ui, |ui| {
                    for d in LoadDuration::ALL {
                        ui.selectable_value(&mut spec.duration, d, d.as_cli());
                    }
                });
            ui.label("Seed");
            ui.add(egui::TextEdit::singleline(&mut spec.seed).desired_width(100.0));
        });
        ui.colored_label(
            theme::muted(),
            "START LOAD may restart with PURGATORY_ADMISSION_CAP=256 (consent).",
        );
    });
    ui.add_space(theme::SECTION_GAP);

    let model = run_view::load_model(snap);
    let start = HubCommand::StartLoad { spec: spec.clone() };
    out.merge(run_view::show_run(
        ui,
        &model,
        run_state,
        start,
        HubCommand::StopLoad,
        |ui, cmd| {
            if ui.button("ANALYZE LAST RUN").clicked() {
                *cmd = Some(HubCommand::AnalyzeLastRun);
            }
            if ui.button("LOAD LOGS").clicked() {
                *cmd = Some(HubCommand::OpenLoadLogs);
            }
            if ui.button("LAST REPORT").clicked() {
                *cmd = Some(HubCommand::OpenLastReport);
            }
        },
    ));
    out
}
