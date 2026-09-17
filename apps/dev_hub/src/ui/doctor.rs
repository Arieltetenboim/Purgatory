use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

use eframe::egui::{self, RichText};
use purgatory_dev_runtime::HubSnapshot;

use crate::theme;
use crate::ui::layout::{self, card};

#[derive(Clone, Debug)]
struct Check {
    label: &'static str,
    ok: bool,
    detail: String,
}

pub fn show(ui: &mut egui::Ui, snap: &HubSnapshot) {
    layout::page_header(
        ui,
        "Environment Doctor",
        "Fast preflight for the local PURGATORY development environment.",
    );

    let checks = collect(snap);
    let passed = checks.iter().filter(|c| c.ok).count();
    card(ui, "Preflight", |ui| {
        ui.label(
            RichText::new(format!("{passed}/{} checks passed", checks.len()))
                .color(if passed == checks.len() { theme::success() } else { theme::destructive() })
                .strong(),
        );
        ui.add_space(8.0);
        egui::Grid::new("doctor_checks")
            .num_columns(3)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for check in checks {
                    ui.colored_label(if check.ok { theme::success() } else { theme::destructive() }, if check.ok { "✓" } else { "×" });
                    ui.label(RichText::new(check.label).strong());
                    ui.add(egui::Label::new(check.detail).wrap());
                    ui.end_row();
                }
            });
    });
}

fn collect(snap: &HubSnapshot) -> Vec<Check> {
    let root = Path::new(&snap.workspace);
    let mut checks = Vec::new();
    checks.push(Check {
        label: "Cargo",
        ok: snap.cargo_found,
        detail: command_version("cargo", &["--version"]).unwrap_or_else(|| "not found on PATH".into()),
    });
    for (label, exe, args) in [
        ("Rustc", "rustc", &["--version"][..]),
        ("Rustup", "rustup", &["--version"][..]),
        ("Git", "git", &["--version"][..]),
        ("PowerShell", "powershell.exe", &["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"][..]),
    ] {
        let detail = command_version(exe, args);
        checks.push(Check { label, ok: detail.is_some(), detail: detail.unwrap_or_else(|| "not found on PATH".into()) });
    }
    for (label, rel) in [
        ("Content", "content"),
        ("Graphics", "Graphic"),
        ("Scripts", "scripts"),
    ] {
        let path = root.join(rel);
        checks.push(Check { label, ok: path.exists(), detail: path.display().to_string() });
    }
    let endpoint_port = snap.endpoint.rsplit(':').next().and_then(|p| p.parse::<u16>().ok());
    if let Some(port) = endpoint_port {
        let available = TcpListener::bind(("127.0.0.1", port)).is_ok();
        let expected_busy = snap.process_alive;
        checks.push(Check {
            label: "Server Port",
            ok: available || expected_busy,
            detail: if available { format!("{port} available") } else if expected_busy { format!("{port} in use by active server") } else { format!("{port} already occupied") },
        });
    }
    checks
}

fn command_version(exe: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}
