use eframe::egui;
use purgatory_dev_runtime::{BuildProfile, HubCommand, HubSnapshot, LogLevel, ServerState};

use crate::theme;
use crate::ui::layout::{self, card, kv_row};

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(ui, "Settings", "Applies to newly spawned processes only.");

    card(ui, "Build profile", |ui| {
        ui.horizontal(|ui| {
            for profile in BuildProfile::ALL {
                let selected = snap.build_profile == profile.as_str();
                if ui.selectable_label(selected, profile.as_str()).clicked() {
                    cmd = Some(HubCommand::SetBuildProfile { profile });
                }
            }
        });
        ui.colored_label(
            theme::muted(),
            "Changing profile does not restart a live server.",
        );
    });
    ui.add_space(theme::SECTION_GAP);

    card(ui, "Log level (new processes)", |ui| {
        ui.horizontal(|ui| {
            for level in LogLevel::ALL {
                let selected = snap.log_level == level;
                if ui.selectable_label(selected, level.as_str()).clicked() {
                    cmd = Some(HubCommand::SetLogLevel { level });
                }
            }
        });
    });
    ui.add_space(theme::SECTION_GAP);

    card(ui, "Environment", |ui| {
        kv_row(ui, "Endpoint", &snap.endpoint);
        kv_row(ui, "Workspace", &snap.workspace);
        kv_row(
            ui,
            "cargo",
            if snap.cargo_found {
                "on PATH"
            } else {
                "NOT FOUND"
            },
        );
        ui.colored_label(
            theme::muted(),
            "PURGATORY_DATA_DIR is set per Runtime Validation / load ExtraEnv, not globally here.",
        );
        if !snap.cargo_found {
            ui.colored_label(
                theme::state_color(ServerState::Degraded),
                "cargo is not on PATH",
            );
        }
    });
    ui.add_space(theme::SECTION_GAP);

    card(ui, "Ops", |ui| {
        ui.horizontal(|ui| {
            if ui.button("QUALITY GATE").clicked() {
                cmd = Some(HubCommand::QualityGate);
            }
            if ui.button("PHASE 7.8 GATE").clicked() {
                cmd = Some(HubCommand::Phase78Gate);
            }
            if snap.phase78_gate_active {
                ui.colored_label(theme::muted(), "(gate running — ladder isolated)");
            }
            if ui.button("REBUILD").clicked() {
                cmd = Some(HubCommand::Rebuild);
            }
            if ui
                .add(egui::Button::new("KILL ALL").fill(egui::Color32::from_rgb(140, 50, 50)))
                .clicked()
            {
                cmd = Some(HubCommand::KillAll);
            }
        });
        ui.colored_label(
            theme::muted(),
            "QUALITY GATE runs scripts/check.ps1. PHASE 7.8 GATE runs scripts/phase_78_gate.ps1 (capacity regression; long). KILL ALL stops server, clients, load, and workspace cargo (unlike closing the Hub).",
        );
    });
    cmd
}
