//! Provisional eframe shell. Orchestration stays in `purgatory-dev-runtime`.

use std::time::{Duration, Instant};

use eframe::egui;
use purgatory_dev_runtime::{LiveHubSession, ValidationSpec};

use crate::navigation::{HubPage, PageKind};
use crate::theme;
use crate::ui;

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([800.0, 560.0])
            .with_title("PURGATORY Developer Hub"),
        ..Default::default()
    };
    eframe::run_native(
        "PURGATORY Developer Hub",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(DevHubApp::new()))
        }),
    )
}

struct DevHubApp {
    session: Result<LiveHubSession, String>,
    page: HubPage,
    validation_spec: ValidationSpec,
}

impl DevHubApp {
    fn new() -> Self {
        Self {
            session: LiveHubSession::open(),
            page: HubPage::Dashboard,
            validation_spec: ValidationSpec::smoke(),
        }
    }
}

impl eframe::App for DevHubApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        if let Ok(session) = &mut self.session {
            session.tick(Instant::now());
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match &mut self.session {
            Err(err) => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.heading("PURGATORY Developer Hub");
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
                });
            }
            Ok(session) => {
                let snap = session.snapshot(Instant::now());
                let mut cmd = None;
                let mut open_logs = false;

                egui::Panel::top("hub_header").show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.heading("PURGATORY Developer Hub");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.colored_label(theme::muted(), "eframe provisional");
                        });
                    });
                    ui.label(format!(
                        "{}   {}   profile {}",
                        snap.identity, snap.workspace, snap.build_profile
                    ));
                    ui.add_space(4.0);
                });

                egui::Panel::bottom("hub_status").show(ui, |ui| {
                    ui.add_space(2.0);
                    ui::status::bar(ui, &snap);
                    ui.add_space(2.0);
                });

                egui::Panel::left("hub_nav")
                    .resizable(true)
                    .default_size(176.0)
                    .min_size(140.0)
                    .show(ui, |ui| {
                        ui.add_space(6.0);
                        ui.strong("Navigate");
                        ui.add_space(4.0);
                        let mut last_group: Option<&str> = None;
                        for page in HubPage::ALL {
                            if page.group() != last_group {
                                if let Some(group) = page.group() {
                                    ui.add_space(6.0);
                                    ui.weak(group);
                                }
                                last_group = page.group();
                            }
                            let mut text = egui::RichText::new(page.label());
                            if page.kind() == PageKind::Placeholder {
                                text = text.color(theme::muted());
                            }
                            if ui.selectable_label(self.page == page, text).clicked() {
                                self.page = page;
                            }
                        }
                    });

                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match self.page {
                            HubPage::Dashboard => {
                                cmd = ui::dashboard::show(ui, &snap);
                            }
                            HubPage::RuntimeServer => {
                                cmd = ui::runtime_server::show(ui, &snap);
                            }
                            HubPage::Logs => {
                                open_logs = ui::logs::show(ui, &snap);
                            }
                            HubPage::Validation => {
                                cmd = ui::validation::show(ui, &snap, &mut self.validation_spec);
                            }
                            other => ui::placeholders::show(ui, other),
                        });
                });

                if let Some(cmd) = cmd {
                    session.command(cmd, Instant::now());
                }
                if open_logs {
                    ui::logs::open_log_dir(&snap.log_dir);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_page_is_dashboard() {
        assert_eq!(HubPage::Dashboard.label(), "Dashboard");
        assert_eq!(HubPage::RuntimeServer.kind(), PageKind::Live);
        assert_eq!(HubPage::Validation.kind(), PageKind::Live);
    }
}
