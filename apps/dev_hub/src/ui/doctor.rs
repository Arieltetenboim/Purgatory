use std::net::TcpListener;
use std::path::Path;

use eframe::egui::{self, RichText};
use purgatory_dev_runtime::{HubSnapshot, ListenerDiag};

use crate::theme;
use crate::ui::layout::card;

#[derive(Clone, Debug)]
struct Check {
    label: &'static str,
    ok: bool,
    detail: String,
}

/// Cheap structural preflight only. Deeper process/version/network probes belong in a
/// background job rather than the egui frame path.
pub fn show_section(ui: &mut egui::Ui, snap: &HubSnapshot) {
    let checks = collect(snap);
    let passed = checks.iter().filter(|c| c.ok).count();
    card(ui, "Environment Doctor", |ui| {
        ui.label(
            RichText::new(format!("{passed}/{} fast checks passed", checks.len()))
                .color(if passed == checks.len() {
                    theme::success()
                } else {
                    theme::destructive()
                })
                .strong(),
        );
        ui.colored_label(
            theme::muted(),
            "Structural preflight: Cargo discovery, required directories, and local server-port state. It does not validate tool versions, file contents, gameplay login, or external/internet reachability.",
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
                        if check.ok { "PASS" } else { "FAIL" },
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
            ok: path.is_dir(),
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
        let (ok, detail) = if snap.process_alive {
            (
                snap.listener != ListenerDiag::No,
                if snap.listener == ListenerDiag::No {
                    format!("{port} server process alive but listener not detected")
                } else {
                    format!("{port} owned by running server")
                },
            )
        } else if available {
            (true, format!("{port} available"))
        } else {
            (false, format!("{port} already occupied"))
        };
        checks.push(Check {
            label: "Server Port",
            ok,
            detail,
        });
    }
    checks
}
