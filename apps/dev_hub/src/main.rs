//! Provisional Developer Hub GUI. All lifecycle logic lives in `purgatory-dev-runtime`.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod ai_modular_reference;
mod app;
mod assets;
mod authoring_template;
mod headwear_side_master;
mod navigation;
mod theme;
mod ui;

fn main() -> eframe::Result {
    app::run()
}
