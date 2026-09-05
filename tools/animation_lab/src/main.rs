//! Animation Lab window.

mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([1100.0, 700.0])
            .with_resizable(true)
            .with_title("PURGATORY Animation Lab"),
        ..Default::default()
    };
    eframe::run_native(
        "PURGATORY Animation Lab",
        options,
        Box::new(|_cc| Ok(Box::new(ui::AnimationLabApp::new()))),
    )
}
