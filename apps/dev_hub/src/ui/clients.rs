use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot};

use crate::theme;
use crate::ui::layout::{self, bounded_log_panel, card, kv_row};

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Runtime / Clients",
        "Native clients launch after Server Ready. Closing the Hub does not stop them.",
    );

    card(ui, "Clients", |ui| {
        kv_row(ui, "Running", snap.client_count.to_string());
        kv_row(ui, "Queued", snap.pending_clients.to_string());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(snap.can_request_clients, egui::Button::new("+ 1"))
                .clicked()
            {
                cmd = Some(HubCommand::RequestClients { count: 1 });
            }
            if ui
                .add_enabled(snap.can_request_clients, egui::Button::new("+ 2"))
                .clicked()
            {
                cmd = Some(HubCommand::RequestClients { count: 2 });
            }
            if ui
                .add_enabled(snap.can_request_clients, egui::Button::new("+ 3"))
                .clicked()
            {
                cmd = Some(HubCommand::RequestClients { count: 3 });
            }
            if ui
                .add_enabled(snap.can_stop_clients, egui::Button::new("STOP ALL"))
                .clicked()
            {
                cmd = Some(HubCommand::StopClients);
            }
        });
        ui.colored_label(theme::muted(), "F6 queues one client.");
    });
    ui.add_space(theme::SECTION_GAP);

    card(ui, "CLIENT LOG", |ui| {
        if bounded_log_panel(ui, "client_log", &snap.client_log_lines, "(empty)") {
            cmd = Some(HubCommand::ClearClientLog);
        }
    });
    cmd
}
