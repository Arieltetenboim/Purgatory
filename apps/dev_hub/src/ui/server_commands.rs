use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};
use purgatory_common::{
    DEFAULT_DEV_ADMIN_PORT, DEV_ADMIN_PORT_ENV, DevAdminRequest, DevAdminResponse, DevAdminSnapshot,
};
use purgatory_dev_runtime::{HubSnapshot, ServerState};

use crate::theme;
use crate::ui::layout::{btn_destructive, btn_ghost, btn_primary, hub_card};

const HISTORY_CAP: usize = 64;
const AUTO_REFRESH_CONNECTED: Duration = Duration::from_secs(2);
const AUTO_REFRESH_UNAVAILABLE: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(2);

type AdminReply = (bool, Result<DevAdminResponse, String>);

pub struct ServerCommandsState {
    snapshot: Option<DevAdminSnapshot>,
    selected_player: Option<u64>,
    selected_npc: Option<u32>,
    selected_item: Option<u32>,
    item_quantity: u32,
    narrative_fact_id: String,
    narrative_fact_value: bool,
    channel: u32,
    history: Vec<String>,
    sender: Sender<AdminReply>,
    receiver: Receiver<AdminReply>,
    in_flight: usize,
    last_refresh: Option<Instant>,
    last_transport_error: Option<String>,
}

impl Default for ServerCommandsState {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            snapshot: None,
            selected_player: None,
            selected_npc: None,
            selected_item: None,
            item_quantity: 1,
            narrative_fact_id: "welcome.workshop.package_delivered".into(),
            narrative_fact_value: true,
            channel: 1,
            history: Vec::new(),
            sender,
            receiver,
            in_flight: 0,
            last_refresh: None,
            last_transport_error: None,
        }
    }
}

impl ServerCommandsState {
    pub fn show(&mut self, ui: &mut egui::Ui, snap: &HubSnapshot) {
        self.poll();
        self.maybe_refresh(snap);

        let _ = hub_card(ui, "CMD", "Server Commands", |ui| {
            ui.horizontal(|ui| {
                let connected = self.snapshot.is_some() && self.last_transport_error.is_none();
                let (label, color) = if connected {
                    ("DEV ADMIN CONNECTED", theme::success())
                } else if snap.process_alive {
                    ("DEV ADMIN UNAVAILABLE", theme::destructive())
                } else {
                    ("SERVER OFFLINE", theme::muted())
                };
                ui.colored_label(color, RichText::new(label).strong());
                if ui
                    .add(btn_ghost("Refresh"))
                    .on_hover_text("Refresh active player connections and DEV command catalog.")
                    .clicked()
                {
                    self.send(DevAdminRequest::Snapshot);
                }
            });

            if let Some(error) = &self.last_transport_error {
                ui.colored_label(theme::destructive(), error);
                ui.colored_label(
                    theme::muted(),
                    "Automatic retries are throttled. Rebuild/restart the server if this binary predates DEV admin support.",
                );
            }
            ui.add_space(8.0);

            let ready = matches!(
                snap.server_state,
                ServerState::Ready | ServerState::Degraded
            );
            let Some(snapshot) = self.snapshot.clone() else {
                ui.colored_label(
                    theme::muted(),
                    "Start the server, then Refresh. Commands are delivered over the local DEV admin channel.",
                );
                return;
            };

            if snapshot.players.is_empty() {
                ui.colored_label(theme::muted(), "No active player connections.");
                return;
            }

            if self.selected_player.is_none_or(|selected| {
                !snapshot.players.iter().any(|p| p.connection_id == selected)
            }) {
                self.selected_player = snapshot.players.first().map(|p| p.connection_id);
            }
            if self
                .selected_npc
                .is_none_or(|selected| !snapshot.npcs.iter().any(|n| n.content_id == selected))
            {
                self.selected_npc = snapshot.npcs.first().map(|n| n.content_id);
            }
            if self.selected_item.is_none_or(|selected| {
                !snapshot
                    .items
                    .iter()
                    .any(|item| item.content_id == selected)
            }) {
                self.selected_item = snapshot.items.first().map(|item| item.content_id);
            }
            if !snapshot.facts.is_empty()
                && !snapshot
                    .facts
                    .iter()
                    .any(|fact| fact == &self.narrative_fact_id)
            {
                self.narrative_fact_id = snapshot.facts[0].clone();
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new("Target Player").color(theme::muted()));
                egui::ComboBox::from_id_salt("server_commands_player")
                    .selected_text(
                        self.selected_player
                            .map(|id| format!("Connection {id}"))
                            .unwrap_or_else(|| "Select player".into()),
                    )
                    .show_ui(ui, |ui| {
                        for player in &snapshot.players {
                            ui.selectable_value(
                                &mut self.selected_player,
                                Some(player.connection_id),
                                format!("Connection {}", player.connection_id),
                            );
                        }
                    });
                ui.colored_label(theme::muted(), "All commands remain server-authoritative.");
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Spawn NPC").strong());
                egui::ComboBox::from_id_salt("server_commands_npc")
                    .width(300.0)
                    .selected_text(selected_content_label(
                        &snapshot.npcs,
                        self.selected_npc,
                        "Select NPC",
                    ))
                    .show_ui(ui, |ui| {
                        for npc in &snapshot.npcs {
                            ui.selectable_value(
                                &mut self.selected_npc,
                                Some(npc.content_id),
                                format!("{}  ({})", npc.authored_id, npc.content_id),
                            );
                        }
                    });
                let enabled = ready && self.selected_player.is_some() && self.selected_npc.is_some();
                let response = ui.add_enabled(enabled, btn_primary("Spawn near player"));
                if response
                    .on_hover_text(
                        "Spawn the selected authored NPC near the selected player's authoritative position.",
                    )
                    .clicked()
                    && let (Some(connection_id), Some(npc_content_id)) =
                        (self.selected_player, self.selected_npc)
                {
                    self.send(DevAdminRequest::SpawnNpc {
                        connection_id,
                        npc_content_id,
                    });
                }
            });

            ui.add_space(7.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Spawn Item").strong());
                egui::ComboBox::from_id_salt("server_commands_item")
                    .width(300.0)
                    .selected_text(selected_content_label(
                        &snapshot.items,
                        self.selected_item,
                        if snapshot.items.is_empty() {
                            "No authored items"
                        } else {
                            "Select item"
                        },
                    ))
                    .show_ui(ui, |ui| {
                        for item in &snapshot.items {
                            ui.selectable_value(
                                &mut self.selected_item,
                                Some(item.content_id),
                                format!("{}  ({})", item.authored_id, item.content_id),
                            );
                        }
                    });
                ui.label("Qty");
                ui.add(egui::DragValue::new(&mut self.item_quantity).range(1..=999));
                let enabled =
                    ready && self.selected_player.is_some() && self.selected_item.is_some();
                let response = ui.add_enabled(enabled, btn_primary("Spawn near player"));
                if response
                    .on_hover_text(
                        "Spawn an authoritative world-drop near the selected player's server position. Quantity must not exceed the authored stack limit.",
                    )
                    .clicked()
                    && let (Some(connection_id), Some(item_content_id)) =
                        (self.selected_player, self.selected_item)
                {
                    self.send(DevAdminRequest::SpawnItem {
                        connection_id,
                        item_content_id,
                        quantity: self.item_quantity,
                    });
                }
            });
            if snapshot.items.is_empty() {
                ui.colored_label(
                    theme::muted(),
                    "No item catalogue received. Rebuild/restart the server to refresh authored items.",
                );
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(RichText::new("Narrative").strong());
            ui.colored_label(
                theme::muted(),
                "Per-player authoritative narrative state. Mutations close an active dialogue before changing its conditions.",
            );
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("Fact");
                egui::ComboBox::from_id_salt("server_commands_fact")
                    .width(300.0)
                    .selected_text(if snapshot.facts.is_empty() {
                        "No authored facts".to_string()
                    } else {
                        self.narrative_fact_id.clone()
                    })
                    .show_ui(ui, |ui| {
                        for fact in &snapshot.facts {
                            ui.selectable_value(
                                &mut self.narrative_fact_id,
                                fact.clone(),
                                fact,
                            );
                        }
                    });
                ui.checkbox(&mut self.narrative_fact_value, "Value");
                let fact_ready =
                    ready && self.selected_player.is_some() && !snapshot.facts.is_empty();
                if ui
                    .add_enabled(fact_ready, btn_primary("Set Fact"))
                    .on_hover_text("Set the selected authored boolean fact for this player.")
                    .clicked()
                    && let Some(connection_id) = self.selected_player
                {
                    self.send(DevAdminRequest::SetNarrativeFact {
                        connection_id,
                        fact_id: self.narrative_fact_id.clone(),
                        value: self.narrative_fact_value,
                    });
                }
                if ui
                    .add_enabled(fact_ready, btn_ghost("Clear Fact"))
                    .on_hover_text(
                        "Remove the selected fact entry for this player; reads fall back to false.",
                    )
                    .clicked()
                    && let Some(connection_id) = self.selected_player
                {
                    self.send(DevAdminRequest::ClearNarrativeFact {
                        connection_id,
                        fact_id: self.narrative_fact_id.clone(),
                    });
                }
            });
            if snapshot.facts.is_empty() {
                ui.colored_label(
                    theme::muted(),
                    "No fact catalogue received. Rebuild/restart the server to use authored fact selection.",
                );
            }
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label("NPC Met");
                let npc_ready = ready && self.selected_player.is_some() && self.selected_npc.is_some();
                if ui
                    .add_enabled(npc_ready, btn_ghost("Mark selected NPC met"))
                    .on_hover_text("Mark the selected authored NPC as met for this player.")
                    .clicked()
                    && let (Some(connection_id), Some(npc_content_id)) =
                        (self.selected_player, self.selected_npc)
                    && let Some(npc) = snapshot
                        .npcs
                        .iter()
                        .find(|entry| entry.content_id == npc_content_id)
                {
                    self.send(DevAdminRequest::MarkNpcMet {
                        connection_id,
                        npc_authored_id: npc.authored_id.clone(),
                    });
                }
                if ui
                    .add_enabled(
                        ready && self.selected_player.is_some(),
                        btn_destructive("Reset Narrative"),
                    )
                    .on_hover_text(
                        "Reset only the selected player's transient narrative state, restore Welcome defaults, and close any active dialogue.",
                    )
                    .clicked()
                    && let Some(connection_id) = self.selected_player
                {
                    self.send(DevAdminRequest::ResetNarrative { connection_id });
                }
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Player").strong());
                if ui
                    .add_enabled(ready, btn_ghost("Reset to Spawn"))
                    .on_hover_text(
                        "Reset the selected player through the existing authoritative DEV reset path.",
                    )
                    .clicked()
                    && let Some(connection_id) = self.selected_player
                {
                    self.send(DevAdminRequest::ResetPlayer { connection_id });
                }

                ui.label("Channel");
                ui.add(egui::DragValue::new(&mut self.channel).range(1..=999));
                if ui
                    .add_enabled(ready, btn_ghost("Apply Channel"))
                    .on_hover_text(
                        "Move the selected player to this DEV channel through server authority.",
                    )
                    .clicked()
                    && let Some(connection_id) = self.selected_player
                {
                    self.send(DevAdminRequest::SetChannel {
                        connection_id,
                        channel: self.channel,
                    });
                }
            });
        });

        ui.add_space(theme::CARD_GAP);
        let _ = hub_card(ui, "LOG", "Server Commands Log", |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.history.is_empty(), btn_ghost("Copy"))
                    .on_hover_text("Copy the entire Server Commands log.")
                    .clicked()
                {
                    ui.ctx().copy_text(self.history.join("\n"));
                }
            });
            if self.history.is_empty() {
                ui.colored_label(theme::muted(), "No Server Commands issued yet.");
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("server_commands_history")
                .max_height(150.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.history {
                        let color = if line.starts_with("V ") {
                            theme::success()
                        } else if line.starts_with("X ") {
                            theme::destructive()
                        } else {
                            theme::body()
                        };
                        ui.colored_label(color, RichText::new(line).font(theme::mono_small()));
                    }
                });
        });
    }

    fn maybe_refresh(&mut self, snap: &HubSnapshot) {
        if !snap.process_alive {
            if self.in_flight == 0 {
                self.snapshot = None;
                self.last_transport_error = None;
                self.last_refresh = None;
            }
            return;
        }
        if self.in_flight > 0 {
            return;
        }
        let interval = if self.last_transport_error.is_some() {
            AUTO_REFRESH_UNAVAILABLE
        } else {
            AUTO_REFRESH_CONNECTED
        };
        let due = self
            .last_refresh
            .is_none_or(|last| last.elapsed() >= interval);
        if due {
            self.send(DevAdminRequest::Snapshot);
        }
    }

    fn send(&mut self, request: DevAdminRequest) {
        let is_snapshot = matches!(request, DevAdminRequest::Snapshot);
        self.in_flight = self.in_flight.saturating_add(1);
        if is_snapshot {
            self.last_refresh = Some(Instant::now());
        }
        let sender = self.sender.clone();
        if std::thread::Builder::new()
            .name("purgatory-dev-admin-client".into())
            .spawn(move || {
                let _ = sender.send((is_snapshot, send_request(&request)));
            })
            .is_err()
        {
            self.in_flight = self.in_flight.saturating_sub(1);
            self.last_transport_error = Some("failed to start DEV admin request worker".into());
        }
    }

    fn poll(&mut self) {
        while let Ok((is_snapshot, result)) = self.receiver.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            match result {
                Ok(DevAdminResponse::Snapshot { snapshot }) => {
                    self.snapshot = Some(snapshot);
                    self.last_transport_error = None;
                }
                Ok(DevAdminResponse::Command { ok, message }) => {
                    self.last_transport_error = None;
                    self.push_history(format!("{} {message}", if ok { 'V' } else { 'X' }));
                    self.last_refresh = None;
                }
                Err(error) => {
                    self.snapshot = None;
                    self.last_transport_error = Some(error.clone());
                    if !is_snapshot {
                        self.push_history(format!("X {error}"));
                    }
                }
            }
        }
    }

    fn push_history(&mut self, line: String) {
        if self.history.len() >= HISTORY_CAP {
            self.history.remove(0);
        }
        self.history.push(line);
    }
}

fn selected_content_label(
    entries: &[purgatory_common::DevAdminContentEntry],
    selected: Option<u32>,
    fallback: &str,
) -> String {
    selected
        .and_then(|id| entries.iter().find(|entry| entry.content_id == id))
        .map(|entry| format!("{}  ({})", entry.authored_id, entry.content_id))
        .unwrap_or_else(|| fallback.into())
}

fn send_request(request: &DevAdminRequest) -> Result<DevAdminResponse, String> {
    let port = std::env::var(DEV_ADMIN_PORT_ENV)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_DEV_ADMIN_PORT);
    let addr = ("127.0.0.1", port)
        .to_socket_addrs()
        .map_err(|err| format!("DEV admin address: {err}"))?
        .next()
        .ok_or_else(|| "DEV admin address unavailable".to_owned())?;
    let mut stream = TcpStream::connect_timeout(&addr, IO_TIMEOUT)
        .map_err(|err| format!("DEV admin connect 127.0.0.1:{port}: {err}"))?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|err| format!("DEV admin read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|err| format!("DEV admin write timeout: {err}"))?;

    let mut body = serde_json::to_vec(request).map_err(|err| format!("encode command: {err}"))?;
    body.push(b'\n');
    stream
        .write_all(&body)
        .map_err(|err| format!("send command: {err}"))?;

    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|err| format!("read command result: {err}"))?;
    if line.is_empty() {
        return Err("DEV admin closed without a response".into());
    }
    serde_json::from_str(line.trim_end()).map_err(|err| format!("decode command result: {err}"))
}
