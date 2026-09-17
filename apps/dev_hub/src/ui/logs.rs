use std::path::PathBuf;

use eframe::egui;
use purgatory_dev_runtime::{HubCommand, HubSnapshot};

use crate::theme;
use crate::ui::layout::{self, card, kv_row};
use crate::ui::log_console;

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
                format!("Showing last {} activity lines", snap.log_lines.len()),
            );
        });
        ui.add_space(4.0);
        if log_console::show(ui, "hub_activity_log", &snap.log_lines, "INFO", "(empty)") {
            clear = Some(HubCommand::ClearActivityLog);
        }
    });

    ui.add_space(theme::CARD_GAP);

    let quality_gate_path = PathBuf::from(&snap.log_dir).join("quality-gate.log");
    let quality_gate_lines = read_recent_lines(&quality_gate_path, 600);
    card(ui, "Quality Gate", |ui| {
        ui.colored_label(
            theme::muted(),
            "scripts/check.ps1 output — runs hidden from Dashboard Quick Actions",
        );
        ui.add_space(4.0);
        if log_console::show(
            ui,
            "quality_gate_log",
            &quality_gate_lines,
            "GATE",
            "No Quality Gate output yet.",
        ) {
            let _ = std::fs::write(&quality_gate_path, "");
        }
    });

    (open, clear)
}

fn read_recent_lines(path: &std::path::Path, max: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max);
    lines[start..]
        .iter()
        .map(|line| (*line).to_owned())
        .collect()
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
