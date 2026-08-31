use eframe::egui;
use purgatory_dev_runtime::HubSnapshot;

use crate::theme;

/// Returns true when OPEN LOGS was clicked.
pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) -> bool {
    let mut open = false;
    ui.heading("Logs");
    ui.label("In-memory activity is bounded. File logs under logs/dev-tools/ may grow on disk.");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(format!("Directory  {}", snap.log_dir));
        if ui.button("OPEN LOGS").clicked() {
            open = true;
        }
    });
    ui.label(format!(
        "Showing last {} of a 4000-line in-memory ring.",
        snap.log_lines.len()
    ));
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            for line in &snap.log_lines {
                ui.colored_label(theme::log_color(line), line);
            }
        });
    open
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
