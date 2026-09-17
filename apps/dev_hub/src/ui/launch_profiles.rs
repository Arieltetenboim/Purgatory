use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot};

use crate::theme;
use crate::ui::layout::{btn_primary, card};

#[derive(Clone, Copy)]
struct Profile { name: &'static str, clients: u32, description: &'static str }

const PROFILES: [Profile; 3] = [
    Profile { name: "Solo", clients: 1, description: "Queue one client for ordinary local development." },
    Profile { name: "2-Client Multiplayer", clients: 2, description: "Queue two clients for multiplayer checks." },
    Profile { name: "3-Client Multiplayer", clients: 3, description: "Queue three clients for replication/load spot checks." },
];

pub fn show_section(ui: &mut egui::Ui, _snap: &HubSnapshot) -> Option<HubCommand> {
    let mut command = None;
    card(ui, "Launch Profiles", |ui| {
        ui.colored_label(theme::muted(), "Repeatable client presets. Requests use the existing authoritative client queue and wait for Server Ready.");
        ui.add_space(8.0);
        for profile in PROFILES {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.strong(profile.name);
                    ui.add(egui::Label::new(profile.description).wrap());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(btn_primary(format!("Queue {}", profile.clients))).on_hover_text("Queue through the existing Hub client lifecycle; this does not create a second launcher path.").clicked() {
                        command = Some(HubCommand::RequestClients { count: profile.clients });
                    }
                });
            });
            ui.add_space(6.0);
        }
    });
    command
}
