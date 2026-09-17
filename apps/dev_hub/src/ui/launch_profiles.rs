use eframe::egui::{self, RichText};
use purgatory_dev_runtime::{HubCommand, HubSnapshot, ServerState};

use crate::theme;
use crate::ui::layout::{self, btn_primary, card};

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    clients: u32,
    description: &'static str,
}

const PROFILES: [Profile; 3] = [
    Profile { name: "Solo Development", clients: 1, description: "Ensure the server is running, then launch one client." },
    Profile { name: "2-Client Multiplayer", clients: 2, description: "Ensure the server is running, then queue two clients for multiplayer checks." },
    Profile { name: "3-Client Multiplayer", clients: 3, description: "Ensure the server is running, then queue three clients for replication/load spot checks." },
];

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut command = None;
    layout::page_header(
        ui,
        "Launch Profiles",
        "Repeatable development presets. Client requests safely queue until Server Ready.",
    );

    for profile in PROFILES {
        card(ui, profile.name, |ui| {
            ui.add(egui::Label::new(profile.description).wrap());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.colored_label(theme::muted(), format!("Server + {} client{}", profile.clients, if profile.clients == 1 { "" } else { "s" }));
                if ui.add(btn_primary("Launch")).on_hover_text("Start the server when needed and queue this profile's clients.").clicked() {
                    // Hub already queues clients until Ready. If the server is stopped, F5/Start is
                    // still the owner of server startup; the profile queues clients immediately.
                    command = Some(HubCommand::RequestClients { count: profile.clients });
                }
            });
            if matches!(snap.server_state, ServerState::Stopped | ServerState::Failed) {
                ui.label(RichText::new("Server is stopped: start it first; queued clients will wait for Ready.").color(theme::muted()));
            }
        });
        ui.add_space(theme::CARD_GAP);
    }
    command
}
