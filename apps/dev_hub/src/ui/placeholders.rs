use eframe::egui;

use crate::navigation::HubPage;
use crate::theme;

pub fn show(ui: &mut egui::Ui, page: HubPage) {
    ui.heading(page.label());
    ui.add_space(6.0);
    ui.label("Not implemented yet.");
    ui.add_space(4.0);
    ui.colored_label(theme::muted(), page.placeholder_blurb());
    ui.add_space(8.0);
    ui.label("This category is reserved in the Hub shell. Slice 1 does not implement it.");
    ui.label("Use PowerShell Developer Tools (DEV.BAT) for capabilities not yet on the Hub.");
}
