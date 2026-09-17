use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot, ServerState};

use crate::theme;
use crate::ui::layout::{self, btn_destructive, btn_ghost, btn_primary, card, kv_row};
use crate::ui::log_console;

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Runtime / Clients",
        "Native clients launch after Server Ready. Closing the Hub does not stop them.",
    );

    card(ui, "Clients", |ui| {
        kv_row(ui, "Running", snap.client_count.to_string());
        let queue_label = if snap.pending_clients > 0 {
            if snap.server_state == ServerState::Ready {
                format!("{} (spawning)", snap.pending_clients)
            } else {
                format!(
                    "{} (waiting for Ready; now {})",
                    snap.pending_clients,
                    snap.server_state.as_str()
                )
            }
        } else {
            "0".into()
        };
        kv_row(ui, "Queued", queue_label);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let response = ui.add_enabled(snap.can_request_clients, btn_primary("+1 Client"));
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Queue one native game client. It launches once the server is Ready.")
                .clicked()
            {
                cmd = Some(HubCommand::RequestClients { count: 1 });
            }

            let response = ui.add_enabled(snap.can_request_clients, btn_ghost("+2 Clients"));
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Queue two native game clients. Launches are staggered after Server Ready.")
                .clicked()
            {
                cmd = Some(HubCommand::RequestClients { count: 2 });
            }

            let response = ui.add_enabled(snap.can_request_clients, btn_ghost("+3 Clients"));
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Queue three native game clients. Launches are staggered after Server Ready.")
                .clicked()
            {
                cmd = Some(HubCommand::RequestClients { count: 3 });
            }

            let response = ui.add_enabled(snap.can_stop_clients, btn_destructive("Stop All"));
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Stop all workspace game clients and clear the pending client queue.")
                .clicked()
            {
                cmd = Some(HubCommand::StopClients);
            }
        });
        ui.colored_label(
            theme::muted(),
            "F6 queues one client. Hover controls for details.",
        );
    });
    ui.add_space(theme::SECTION_GAP);

    card(ui, "Client Log", |ui| {
        if log_console::show(
            ui,
            "client_log_console",
            &snap.client_log_lines,
            "CLIENT",
            "(empty — launch a client or wait for output)",
        ) {
            cmd = Some(HubCommand::ClearClientLog);
        }
    });
    cmd
}
