use eframe::egui::{self, Align, Layout, RichText};
use purgatory_dev_runtime::{BuildProfile, HubCommand, HubSnapshot, LogLevel};

use crate::theme;
use crate::ui::layout::{self, card};

#[derive(Debug, Default)]
pub struct SettingsState;

pub fn show(
    ui: &mut egui::Ui,
    snap: &HubSnapshot,
    _state: &mut SettingsState,
) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Settings",
        "Hub-owned launch defaults. Changes affect newly launched processes and never restart a live process implicitly.",
    );

    card(ui, "Launch Defaults", |ui| {
        ui.label(
            RichText::new("Build Profile")
                .font(theme::section_font())
                .color(theme::body())
                .strong(),
        );
        ui.colored_label(
            theme::muted(),
            "Select the build profile used by Hub-managed launches.",
        );
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            for profile in BuildProfile::ALL {
                let selected = snap.build_profile == profile.as_str();
                if ui.selectable_label(selected, profile.as_str()).clicked() {
                    cmd = Some(HubCommand::SetBuildProfile { profile });
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                effect_badge(ui, "NEW PROCESSES");
            });
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(12.0);

        ui.label(
            RichText::new("Log Level")
                .font(theme::section_font())
                .color(theme::body())
                .strong(),
        );
        ui.colored_label(
            theme::muted(),
            "Select the default log verbosity injected into Hub-managed launches.",
        );
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            for level in LogLevel::ALL {
                let selected = snap.log_level == level;
                if ui.selectable_label(selected, level.as_str()).clicked() {
                    cmd = Some(HubCommand::SetLogLevel { level });
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                effect_badge(ui, "NEW PROCESSES");
            });
        });
    });

    ui.add_space(theme::SECTION_GAP);
    card(ui, "Scope", |ui| {
        ui.colored_label(
            theme::muted(),
            "These are the only real Hub settings currently owned by the runtime. Diagnostics, versions, validation, and process actions intentionally live on their own pages instead of being duplicated here.",
        );
    });

    cmd
}

fn effect_badge(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(theme::nav_active_fill())
        .stroke(theme::card_stroke())
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .font(theme::mono_small())
                    .color(theme::info())
                    .strong(),
            );
        });
}
