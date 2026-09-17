use eframe::egui::{self, Align, Layout, RichText};
use purgatory_dev_runtime::{BuildProfile, HubCommand, HubSnapshot, LogLevel};

use crate::theme;
use crate::ui::layout::{self, card};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SettingsSection {
    #[default]
    BuildLaunch,
    Logging,
}

impl SettingsSection {
    const ALL: [Self; 2] = [Self::BuildLaunch, Self::Logging];

    fn label(self) -> &'static str {
        match self {
            Self::BuildLaunch => "Build & Launch",
            Self::Logging => "Logging",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::BuildLaunch => "Build profile used by Hub-managed launches.",
            Self::Logging => "Log verbosity for newly launched processes.",
        }
    }
}

#[derive(Debug, Default)]
pub struct SettingsState {
    query: String,
    section: SettingsSection,
}

pub fn show(
    ui: &mut egui::Ui,
    snap: &HubSnapshot,
    state: &mut SettingsState,
) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Settings",
        "Configuration only. Runtime actions, diagnostics, and version inventory live with their owning surfaces.",
    );

    ui.horizontal(|ui| {
        ui.label(RichText::new("Search").color(theme::muted()));
        ui.add_sized(
            [360.0, theme::BTN_MIN_H],
            egui::TextEdit::singleline(&mut state.query)
                .hint_text("build, profile, log, trace..."),
        );
        if !state.query.is_empty() && ui.small_button("Clear").clicked() {
            state.query.clear();
        }
    });
    ui.add_space(theme::SECTION_GAP);

    ui.horizontal_top(|ui| {
        egui::Frame::new()
            .fill(theme::card_fill_elevated())
            .stroke(theme::card_stroke())
            .corner_radius(theme::CARD_RADIUS)
            .inner_margin(theme::CARD_PAD)
            .show(ui, |ui| {
                ui.set_min_width(178.0);
                ui.set_max_width(178.0);
                ui.label(
                    RichText::new("Categories")
                        .font(theme::section_font())
                        .color(theme::muted())
                        .strong(),
                );
                ui.add_space(5.0);
                for section in SettingsSection::ALL {
                    let selected = state.section == section && state.query.is_empty();
                    if ui
                        .selectable_label(selected, section.label())
                        .on_hover_text(section.description())
                        .clicked()
                    {
                        state.section = section;
                        state.query.clear();
                    }
                    ui.add_space(2.0);
                }
            });

        ui.add_space(theme::SECTION_GAP);
        ui.vertical(|ui| {
            ui.set_min_width(ui.available_width());
            let query = state.query.trim().to_ascii_lowercase();
            let searching = !query.is_empty();
            let show_build = !searching
                && state.section == SettingsSection::BuildLaunch
                || searching
                    && matches_query(
                        &query,
                        &[
                            "build",
                            "launch",
                            "profile",
                            "debug",
                            "release",
                            "new processes",
                        ],
                    );
            let show_logging = !searching
                && state.section == SettingsSection::Logging
                || searching
                    && matches_query(
                        &query,
                        &["logging", "log", "level", "debug", "trace", "new processes"],
                    );

            if show_build {
                settings_section_header(
                    ui,
                    "Build & Launch",
                    "Settings that affect binaries selected for future Hub-managed process launches.",
                );
                card(ui, "Build Profile", |ui| {
                    setting_description(
                        ui,
                        "Choose the target profile used by newly launched Hub-managed binaries. Running processes are not restarted.",
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
                });
            }

            if show_build && show_logging {
                ui.add_space(theme::SECTION_GAP);
            }

            if show_logging {
                settings_section_header(
                    ui,
                    "Logging",
                    "Logging defaults injected into processes launched after the change.",
                );
                card(ui, "Log Level", |ui| {
                    setting_description(
                        ui,
                        "Controls Hub-provided logging environment for newly launched processes. Existing processes keep their current environment.",
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
            }

            if searching && !show_build && !show_logging {
                card(ui, "No matching settings", |ui| {
                    ui.colored_label(
                        theme::muted(),
                        "No existing Hub setting matches this search. The page does not invent placeholder controls.",
                    );
                });
            }
        });
    });

    ui.add_space(theme::SECTION_GAP);
    ui.colored_label(
        theme::muted(),
        "Current Hub 0.5 settings apply to this Hub session and newly launched processes. They never mutate or restart a live process implicitly.",
    );

    cmd
}

fn matches_query(query: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| term.contains(query))
}

fn settings_section_header(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(
        RichText::new(title)
            .font(theme::state_font())
            .color(theme::body())
            .strong(),
    );
    ui.label(
        RichText::new(description)
            .font(theme::subtitle_font())
            .color(theme::muted()),
    );
    ui.add_space(5.0);
}

fn setting_description(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::subtitle_font())
            .color(theme::muted()),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_cover_only_real_settings() {
        assert!(matches_query("profile", &["build", "profile"]));
        assert!(matches_query("trace", &["log", "trace"]));
        assert!(!matches_query("theme", &["build", "profile", "log", "trace"]));
    }
}
