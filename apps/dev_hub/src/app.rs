//! Provisional eframe shell. Orchestration stays in `purgatory-dev-runtime`.

use std::time::{Duration, Instant};

use eframe::egui::{self, TextureHandle};
use purgatory_dev_runtime::{HubCommand, LiveHubSession, LoadSpec, ValidationSpec};

use crate::assets;
use crate::navigation::{HubPage, PageKind};
use crate::theme;
use crate::ui;
use crate::ui::layout::{chip, nav_item};
use crate::ui::run_view::RunViewState;

pub fn run() -> eframe::Result {
    // Fixed window: Hub layout is authored for this size. Resize caused card
    // overlap / clipped text; keep size stable until a full responsive pass exists.
    const W: f32 = 1280.0;
    const H: f32 = 800.0;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([W, H])
            .with_min_inner_size([W, H])
            .with_max_inner_size([W, H])
            .with_resizable(false)
            .with_title("PURGATORY Developer Hub"),
        ..Default::default()
    };
    eframe::run_native(
        "PURGATORY Developer Hub",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(DevHubApp::new(&cc.egui_ctx)))
        }),
    )
}

struct DevHubApp {
    session: Result<LiveHubSession, String>,
    page: HubPage,
    validation_spec: ValidationSpec,
    load_spec: LoadSpec,
    validation_run: RunViewState,
    load_run: RunViewState,
    authoring_export_status: Option<String>,
    logo: Option<TextureHandle>,
}

impl DevHubApp {
    fn new(ctx: &egui::Context) -> Self {
        Self {
            session: LiveHubSession::open(),
            page: HubPage::Dashboard,
            validation_spec: ValidationSpec::smoke(),
            load_spec: LoadSpec::default(),
            validation_run: RunViewState::default(),
            load_run: RunViewState::default(),
            authoring_export_status: None,
            logo: assets::load_logo(ctx),
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
                    ui.colored_label(theme::destructive(), err);
                });
            }
            Ok(session) => {
                let snap = session.snapshot(Instant::now());
                let mut cmd = None;
                let mut open_logs = false;

                ui.input(|i| {
                    if i.key_pressed(egui::Key::F5) {
                        cmd = Some(if snap.can_start {
                            HubCommand::Start
                        } else {
                            HubCommand::Restart
                        });
                    } else if i.key_pressed(egui::Key::F6) {
                        cmd = Some(HubCommand::RequestClients { count: 1 });
                    }
                });

                egui::Panel::top("hub_header")
                    .exact_size(44.0)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::panel())
                            .inner_margin(egui::Margin::symmetric(12, 6)),
                    )
                    .show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.label(
                                egui::RichText::new("Developer Hub")
                                    .font(theme::section_font())
                                    .strong()
                                    .color(theme::body()),
                            );
                            ui.add_space(10.0);
                            chip(ui, &format!("Phase {}", snap.phase));
                            ui.add_space(4.0);
                            chip(ui, &snap.build_profile);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.colored_label(
                                        theme::muted(),
                                        egui::RichText::new("eframe provisional")
                                            .font(theme::subtitle_font()),
                                    );
                                },
                            );
                        });
                    });

                egui::Panel::bottom("hub_status")
                    .exact_size(28.0)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::panel())
                            .inner_margin(egui::Margin::symmetric(12, 4)),
                    )
                    .show(ui, |ui| {
                        ui::status::bar(ui, &snap);
                    });

                egui::Panel::left("hub_nav")
                    .resizable(true)
                    .default_size(theme::SIDEBAR_WIDTH)
                    .min_size(theme::SIDEBAR_MIN)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::panel())
                            .inner_margin(egui::Margin::symmetric(10, 10)),
                    )
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.vertical_centered(|ui| {
                                assets::paint_logo(
                                    ui,
                                    self.logo.as_ref(),
                                    ui.available_width().min(176.0),
                                    72.0,
                                    "PURGATORY",
                                );
                                ui.label(
                                    egui::RichText::new("Developer Hub")
                                        .font(theme::subtitle_font())
                                        .color(theme::muted()),
                                );
                            });
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);

                            egui::ScrollArea::vertical()
                                .id_salt("hub_nav_scroll")
                                .max_height(ui.available_height() - 56.0)
                                .show(ui, |ui| {
                                    let mut last_group: Option<&str> = None;
                                    for page in HubPage::ALL {
                                        if page.group() != last_group {
                                            if let Some(group) = page.group() {
                                                if last_group.is_some() {
                                                    ui.add_space(12.0);
                                                }
                                                ui.label(
                                                    egui::RichText::new(group)
                                                        .font(theme::subtitle_font())
                                                        .color(theme::muted())
                                                        .strong(),
                                                );
                                                ui.add_space(4.0);
                                            }
                                            last_group = page.group();
                                        }
                                        let selected = self.page == page;
                                        let label = if page.kind() == PageKind::Placeholder {
                                            format!("{} ·", page.label())
                                        } else {
                                            page.label().to_string()
                                        };
                                        if nav_item(ui, selected, page.symbol(), &label).clicked() {
                                            self.page = page;
                                        }
                                        ui.add_space(2.0);
                                    }
                                });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(6.0);
                            ui::status::connection_strip(ui, &snap);
                        });
                    });

                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme::bg()).inner_margin(
                        egui::Margin::symmetric(theme::PAGE_MARGIN as i8, theme::PAGE_MARGIN as i8),
                    ))
                    .show(ui, |ui| {
                        // Vertical page scroll only. Log panels own their own both-axis scroll
                        // so long lines do not get clipped by the page scroller.
                        egui::ScrollArea::vertical()
                            .id_salt("hub_central_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.set_max_width(ui.available_width());
                                match self.page {
                                    HubPage::Dashboard => {
                                        let outcome = ui::dashboard::show(ui, &snap);
                                        if let Some(c) = outcome.command {
                                            cmd = Some(c);
                                        }
                                        if let Some(page) = outcome.navigate {
                                            self.page = page;
                                        }
                                    }
                                    HubPage::RuntimeServer => {
                                        if let Some(c) = ui::runtime_server::show(ui, &snap) {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::RuntimeClients => {
                                        if let Some(c) = ui::clients::show(ui, &snap) {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::Logs => {
                                        let (open, clear_cmd) = ui::logs::show(ui, &snap);
                                        open_logs = open;
                                        if let Some(c) = clear_cmd {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::Validation => {
                                        let outcome = ui::validation::show(
                                            ui,
                                            &snap,
                                            &mut self.validation_spec,
                                            &mut self.validation_run,
                                        );
                                        if let Some(c) = outcome.command {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::Performance => {
                                        let outcome = ui::performance::show(
                                            ui,
                                            &snap,
                                            &mut self.load_spec,
                                            &mut self.load_run,
                                        );
                                        if let Some(c) = outcome.command {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::Phase7Stats => {
                                        let outcome = ui::phase7_stats::show(ui, &snap);
                                        if let Some(c) = outcome.command {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::Settings => {
                                        if let Some(c) = ui::settings::show(ui, &snap) {
                                            cmd = Some(c);
                                        }
                                    }
                                    HubPage::Content => {
                                        if let Some(c) =
                                            ui::content::show(ui, &mut self.authoring_export_status)
                                        {
                                            cmd = Some(c);
                                        }
                                    }
                                    other => ui::placeholders::show(ui, other),
                                }
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
        assert_eq!(HubPage::RuntimeClients.kind(), PageKind::Live);
        assert_eq!(HubPage::Validation.kind(), PageKind::Live);
        assert_eq!(HubPage::Performance.kind(), PageKind::Live);
        assert_eq!(HubPage::Phase7Stats.kind(), PageKind::Live);
        assert_eq!(HubPage::Settings.kind(), PageKind::Live);
    }
}
