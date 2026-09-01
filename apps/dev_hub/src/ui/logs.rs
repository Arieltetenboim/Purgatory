use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot};

use crate::theme;
use crate::ui::layout::{self, card, kv_row, log_panel};

/// Returns (open_log_dir, optional clear command).
pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> (bool, Option<HubCommand>) {
    let mut open = false;
    let mut clear = None;
    layout::page_header(
        ui,
        "Logs",
        "In-memory activity is bounded. File logs under logs/dev-tools/ may grow on disk.",
    );

    card(ui, "Activity", |ui| {
        kv_row(ui, "Directory", &snap.log_dir);
        ui.horizontal(|ui| {
            if ui.button("OPEN LOGS").clicked() {
                open = true;
            }
            ui.colored_label(
                theme::muted(),
                format!("Showing last {} of a 4000-line ring", snap.log_lines.len()),
            );
        });
        ui.add_space(4.0);
        if log_panel(
            ui,
            "hub_activity_log",
            &snap.log_lines,
            "(empty)",
            360.0,
            "scroll ↕↔ for long lines",
        ) {
            clear = Some(HubCommand::ClearActivityLog);
        }
    });
    (open, clear)
}

pub fn open_log_dir(dir: &str) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}
