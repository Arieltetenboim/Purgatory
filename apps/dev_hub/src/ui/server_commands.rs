use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};
use purgatory_common::{
    DEFAULT_DEV_ADMIN_PORT, DEV_ADMIN_PORT_ENV, DevAdminRequest, DevAdminResponse,
    DevAdminSnapshot,
};
use purgatory_dev_runtime::{HubSnapshot, ServerState};

use crate::theme;
use crate::ui::layout::{btn_ghost, btn_primary, hub_card};

const HISTORY_CAP: usize = 64;
const AUTO_REFRESH: Duration = Duration::from_secs(2);
const IO_TIMEOUT: Duration = Duration::from_secs(2);

pub struct ServerCommandsState {
    snapshot: Option<DevAdminSnapshot>,
    selected_player: Option<u64>,
    selected_npc: Option<u32>,
    channel: u32,
    speed_hundredths: u16,
    jump_hundredths: u16,
    history: Vec<String>,
    sender: Sender<Result<DevAdminResponse, String>>,
    receiver: Receiver<Result<DevAdminResponse, String>>,
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
            channel: 1,
            speed_hundredths: 500,
            jump_hundredths: 900,
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

        let _ = hub_card(ui, "⌘", "Server Commands", |ui| {
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
            }
            ui.add_space(8.0);

            let ready = matches!(snap.server_state, ServerState::Ready | ServerState::Degraded);
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

            if self
                .selected_player
                .is_none_or(|selected| !snapshot.players.iter().any(|p| p.connection_id == selected))
            {
                self.selected_player = snapshot.players.first().map(|p| p.connection_id);
            }
            if self
                .selected_npc
                .is_none_or(|selected| !snapshot.npcs.iter().any(|n| n.content_id == selected))
            {
                self.selected_npc = snapshot.npcs.first().map(|n| n.content_id);
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
                ui.colored_label(
                    theme::muted(),
                    "All commands remain server-authoritative.",
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Spawn NPC").strong());
                egui::ComboBox::from_id_salt("server_commands_npc")
                    .width(300.0)
                    .selected_text(selected_npc_label(&snapshot, self.selected_npc))
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
                {
                    if let (Some(connection_id), Some(npc_content_id)) =
                        (self.selected_player, self.selected_npc)
                    {
                        self.send(DevAdminRequest::SpawnNpc {
                            connection_id,
                            npc_content_id,
                        });
                    }
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
                {
                    if let Some(connection_id) = self.selected_player {
                        self.send(DevAdminRequest::ResetPlayer { connection_id });
                    }
                }

                ui.label("Channel");
                ui.add(egui::DragValue::new(&mut self.channel).range(1..=999));
                if ui
                    .add_enabled(ready, btn_ghost("Apply Channel"))
                    .on_hover_text("Move the selected player to this DEV channel through server authority.")
                    .clicked()
                {
                    if let Some(connection_id) = self.selected_player {
                        self.send(DevAdminRequest::SetChannel {
                            connection_id,
                            channel: self.channel,
                        });
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Movement overrides").strong());
                ui.label("Speed ×0.01");
                ui.add(egui::DragValue::new(&mut self.speed_hundredths).range(50..=2400));
                if ui
                    .add_enabled(ready, btn_ghost("Set Speed"))
                    .on_hover_text("Apply the server-side DEV speed override to the selected player.")
                    .clicked()
                {
                    if let Some(connection_id) = self.selected_player {
                        self.send(DevAdminRequest::SetSpeed {
                            connection_id,
                            hundredths: Some(self.speed_hundredths),
                        });
                    }
                }
                if ui
                    .add_enabled(ready, btn_ghost("Clear Speed"))
                    .on_hover_text("Remove the DEV speed override and return to authored/default movement speed.")
                    .clicked()
                {
                    if let Some(connection_id) = self.selected_player {
                        self.send(DevAdminRequest::SetSpeed {
                            connection_id,
                            hundredths: None,
                        });
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Jump ×0.01");
                ui.add(egui::DragValue::new(&mut self.jump_hundredths).range(100..=3000));
                if ui
                    .add_enabled(ready, btn_ghost("Set Jump"))
                    .on_hover_text("Apply the server-side DEV jump override to the selected player.")
                    .clicked()
                {
                    if let Some(connection_id) = self.selected_player {
                        self.send(DevAdminRequest::SetJump {
                            connection_id,
                            hundredths: Some(self.jump_hundredths),
                        });
                    }
                }
                if ui
                    .add_enabled(ready, btn_ghost("Clear Jump"))
                    .on_hover_text("Remove the DEV jump override and return to authored/default jump power.")
                    .clicked()
                {
                    if let Some(connection_id) = self.selected_player {
                        self.send(DevAdminRequest::SetJump {
                            connection_id,
                            hundredths: None,
                        });
                    }
                }
            });

            ui.add_space(8.0);
            ui.colored_label(
                theme::muted(),
                "This first slice exposes existing safe DEV hooks. Item spawning and narrative-state mutation require explicit simulation-owner commands and are not faked here.",
            );
        });

        ui.add_space(theme::CARD_GAP);
        let _ = hub_card(ui, "≡", "Server Commands Log", |ui| {
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
                        let color = if line.starts_with('✓') {
                            theme::success()
                        } else if line.starts_with('×') {
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
        if !snap.process_alive || self.in_flight > 0 {
            return;
        }
        let due = self
            .last_refresh
            .is_none_or(|last| last.elapsed() >= AUTO_REFRESH);
        if due {
            self.send(DevAdminRequest::Snapshot);
        }
    }

    fn send(&mut self, request: DevAdminRequest) {
        self.in_flight = self.in_flight.saturating_add(1);
        if matches!(request, DevAdminRequest::Snapshot) {
            self.last_refresh = Some(Instant::now());
        }
        let sender = self.sender.clone();
        std::thread::Builder::new()
            .name("purgatory-dev-admin-client".into())
            .spawn(move || {
                let _ = sender.send(send_request(&request));
            })
            .ok();
    }

    fn poll(&mut self) {
        while let Ok(result) = self.receiver.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            match result {
                Ok(DevAdminResponse::Snapshot { snapshot }) => {
                    self.snapshot = Some(snapshot);
                    self.last_transport_error = None;
                }
                Ok(DevAdminResponse::Command { ok, message }) => {
                    self.last_transport_error = None;
                    self.push_history(format!("{} {message}", if ok { '✓' } else { '×' }));
                    self.last_refresh = None;
                }
                Err(error) => {
                    self.snapshot = None;
                    self.last_transport_error = Some(error.clone());
                    self.push_history(format!("× {error}"));
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

fn selected_npc_label(snapshot: &DevAdminSnapshot, selected: Option<u32>) -> String {
    selected
        .and_then(|id| snapshot.npcs.iter().find(|npc| npc.content_id == id))
        .map(|npc| format!("{}  ({})", npc.authored_id, npc.content_id))
        .unwrap_or_else(|| "Select NPC".into())
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
