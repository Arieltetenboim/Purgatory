use eframe::egui;
use purgatory_dev_runtime::{BuildProfile, HubCommand, HubSnapshot, LogLevel, ServerState};

use crate::theme;
use crate::ui::layout::{self, card, kv_row};
use crate::ui::{doctor, launch_profiles, tool_launch};

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Settings",
        "Development environment, launch presets, and process defaults.",
    );

    if let Some(profile_cmd) = launch_profiles::show_section(ui, snap) {
        cmd = Some(profile_cmd);
    }
    ui.add_space(theme::SECTION_GAP);

    doctor::show_section(ui, snap);
    ui.add_space(theme::SECTION_GAP);

    versions_card(ui);
    ui.add_space(theme::SECTION_GAP);

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
            if ui
                .button("QUALITY GATE")
                .on_hover_text(
                    "Run the same hidden Quality Gate used by Dashboard; progress is shown on Dashboard and detailed output in Logs.",
                )
                .clicked()
            {
                let _ = tool_launch::launch_quality_gate();
            }
            if ui
                .button("REBUILD")
                .on_hover_text("Rebuild Hub-managed binaries using the existing runtime build path.")
                .clicked()
            {
                cmd = Some(HubCommand::Rebuild);
            }
            if ui
                .add(
                    egui::Button::new("KILL ALL")
                        .fill(egui::Color32::from_rgb(140, 50, 50)),
                )
                .on_hover_text("Emergency cleanup of workspace runtime processes. Shortcut: F9.")
                .clicked()
            {
                cmd = Some(HubCommand::KillAll);
            }
        });
        ui.colored_label(
            theme::muted(),
            "Quality Gate runs hidden with isolated output. Kill All stops server, clients, load, and workspace cargo.",
        );
    });
    cmd
}

fn versions_card(ui: &mut egui::Ui) {
    const COMPONENTS: [&str; 6] = [
        "Client",
        "Server",
        "Developer Hub",
        "Animation Lab",
        "NPC Lab",
        "Character Part Lab",
    ];

    card(ui, "Component Versions", |ui| {
        egui::Grid::new("component_versions")
            .num_columns(2)
            .spacing([18.0, 5.0])
            .show(ui, |ui| {
                for component in COMPONENTS {
                    ui.label(component);
                    ui.colored_label(theme::muted(), "not assigned");
                    ui.end_row();
                }
            });
        ui.add_space(4.0);
        ui.colored_label(
            theme::muted(),
            "Placeholders until each component owns an explicit version file. Future version bumps should accompany changes that alter that component's tracked behavior or contract.",
        );
    });
}
