use eframe::egui;
use purgatory_dev_runtime::HubCommand;

use crate::ai_modular_reference;
use crate::authoring_template;
use crate::headwear_side_master;
use crate::theme;
use crate::ui::layout::{self, btn_ghost, btn_primary, card};

pub fn show(ui: &mut egui::Ui, export_status: &mut Option<String>) -> Option<HubCommand> {
    let mut cmd = None;
    layout::page_header(
        ui,
        "Content",
        "Standalone authoring tools. Animation Lab is A7.0; map/NPC editors stay later.",
    );
    card(ui, "Animation Lab", |ui| {
        ui.label(
            "Opens a separate window. Lifetime is independent of the Hub and the game client.",
        );
        ui.add_space(6.0);
        ui.colored_label(
            theme::muted(),
            "Authors A6 .anim files. Saved clips reach the game after a client rebuild.",
        );
        ui.add_space(8.0);
        if ui.add(btn_primary("Launch Animation Lab")).clicked() {
            cmd = Some(HubCommand::LaunchAnimationLab);
        }
    });
    ui.add_space(12.0);
    card(ui, "Humanoid v0 authoring template", |ui| {
        ui.label(
            "Exports a transparent SVG from live Humanoid v0 bind / slot-rest / anchor contracts. Photoshop reference only — not a runtime asset.",
        );
        ui.add_space(6.0);
        ui.colored_label(
            theme::muted(),
            format!(
                "{}×{} px · 1 wu = {} px · origin ({}, {}) = world (0,0)",
                authoring_template::CANVAS_W,
                authoring_template::CANVAS_H,
                authoring_template::PX_PER_WU,
                authoring_template::ORIGIN_X,
                authoring_template::ORIGIN_Y,
            ),
        );
        let crown_w = ai_modular_reference::crown_world();
        let crown_px = ai_modular_reference::crown_assembled_canvas();
        ui.colored_label(
            theme::muted(),
            format!(
                "AI sheet {}×{} px · same {} px/wu · Crown world ({:.3}, {:.3}) · assembled ({:.1}, {:.1})",
                ai_modular_reference::CANVAS_W,
                ai_modular_reference::CANVAS_H,
                authoring_template::PX_PER_WU,
                crown_w[0],
                crown_w[1],
                crown_px[0],
                crown_px[1],
            ),
        );
        ui.add_space(8.0);
        if ui.add(btn_primary("Export authoring template")).clicked() {
            let path = authoring_template::default_output_path();
            *export_status = Some(match authoring_template::export_to(&path) {
                Ok(written) => format!("Wrote {}", written.display()),
                Err(err) => format!("Export failed: {err}"),
            });
        }
        ui.add_space(6.0);
        if ui.add(btn_ghost("Export AI modular reference")).clicked() {
            let path = ai_modular_reference::default_output_path();
            *export_status = Some(match ai_modular_reference::export_to(&path) {
                Ok(written) => format!("Wrote {}", written.display()),
                Err(err) => format!("Export failed: {err}"),
            });
        }
    });
    ui.add_space(12.0);
    card(ui, "Headwear Side master", |ui| {
        ui.label(
            "Empty 2×2 SVG overlay. Crown + is the attachment point. Extract crops an artist PNG by grid only. Not loaded by the game.",
        );
        ui.add_space(6.0);
        ui.colored_label(
            theme::muted(),
            format!(
                "{}×{} px · {}×{} cells of {}×{} · Crown local ({}, {}) · {} px/wu",
                headwear_side_master::SHEET_W,
                headwear_side_master::SHEET_H,
                headwear_side_master::COLUMNS,
                headwear_side_master::ROWS,
                headwear_side_master::CELL_W,
                headwear_side_master::CELL_H,
                headwear_side_master::CROWN_LOCAL_X,
                headwear_side_master::CROWN_LOCAL_Y,
                authoring_template::PX_PER_WU as u32,
            ),
        );
        ui.add_space(8.0);
        if ui.add(btn_primary("Export Headwear Side master")).clicked() {
            let path = headwear_side_master::default_output_path();
            *export_status = Some(match headwear_side_master::export_to(&path) {
                Ok(written) => format!("Wrote {}", written.display()),
                Err(err) => format!("Export failed: {err}"),
            });
        }
        ui.add_space(6.0);
        if ui.add(btn_ghost("Extract Headwear Side cells")).clicked() {
            *export_status = Some(
                match headwear_side_master::extract_from_png(
                    &headwear_side_master::sheet_png_path(),
                    &headwear_side_master::extracted_dir(),
                ) {
                    Ok(written) => format!("Extracted {}", written.display()),
                    Err(err) => format!("Extract failed: {err}"),
                },
            );
        }
    });
    if let Some(status) = export_status.as_ref() {
        ui.add_space(8.0);
        ui.colored_label(theme::muted(), status);
    }
    cmd
}
