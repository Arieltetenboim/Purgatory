//! Provisional Developer Hub GUI. All lifecycle logic lives in `purgatory-dev-runtime`.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod app;
mod navigation;
mod theme;
mod ui;

fn main() -> eframe::Result {
    app::run()
}
