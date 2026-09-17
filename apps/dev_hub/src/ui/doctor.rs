use std::net::TcpListener;
use std::path::Path;

use eframe::egui::{self, RichText};
use purgatory_dev_runtime::HubSnapshot;

use crate::theme;
use crate::ui::layout::card;

#[derive(Clone, Debug)]
struct Check {
    label: &'static str,
    ok: bool,
    detail: String,
}

/// Cheap preflight only: this runs in the egui frame path, so it deliberately avoids
/// spawning version commands or doing expensive process discovery. Deeper probes belong
/// in a background runtime job, not in rendering.
pub fn show_section(ui: &mut egui::Ui, snap: &HubSnapshot) {
    let checks = collect(snap);
    let passed = checks.iter().filter(|c| c.ok).count();
    card(ui, "Environment Doctor", |ui| {
        ui.label(
            RichText::new(format!("{passed}/{} preflight checks passed", checks.len()))
                .color(if passed == checks.len() {
                    theme::success()
                } else {
                    theme::destructive()
                })
                .strong(),
        );
        ui.colored_label(
            theme::muted(),
            "Fast checks only; no shell commands run from the UI frame.",
        );
        ui.add_space(8.0);
        egui::Grid::new("doctor_checks")
            .num_columns(3)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for check in checks {
                    ui.colored_label(
                        if check.ok {
                            theme::success()
                        } else {
                            theme::destructive()
                        },
                        if check.ok { "✓" } else { "×" },
                    );
                    ui.label(RichText::new(check.label).strong());
                    ui.add(egui::Label::new(check.detail).wrap());
                    ui.end_row();
                }
            });
    });
}

fn collect(snap: &HubSnapshot) -> Vec<Check> {
    let root = Path::new(&snap.workspace);
    let mut checks = vec![Check {
        label: "Cargo",
        ok: snap.cargo_found,
        detail: if snap.cargo_found {
            "available on PATH".into()
        } else {
            "not found on PATH".into()
        },
    }];

    for (label, rel) in [
        ("Content", "content"),
        ("Graphics", "Graphic"),
        ("Scripts", "scripts"),
    ] {
        let path = root.join(rel);
        checks.push(Check {
            label,
            ok: path.exists(),
            detail: path.display().to_string(),
        });
    }

    if let Some(port) = snap
        .endpoint
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
    {
        let available = TcpListener::bind(("127.0.0.1", port)).is_ok();
        let expected_busy = snap.process_alive;
        checks.push(Check {
            label: "Server Port",
            ok: available || expected_busy,
            detail: if available {
                format!("{port} available")
            } else if expected_busy {
                format!("{port} in use by active server")
            } else {
                format!("{port} already occupied")
            },
        });
    }
    checks
}
