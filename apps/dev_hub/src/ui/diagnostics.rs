use eframe::egui;
use purgatory_dev_runtime::HubSnapshot;

use crate::ui::{doctor, layout};

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) {
    layout::page_header(
        ui,
        "Diagnostics",
        "Environment health and structural checks for this development workspace.",
    );
    doctor::show_section(ui, snap);
}
