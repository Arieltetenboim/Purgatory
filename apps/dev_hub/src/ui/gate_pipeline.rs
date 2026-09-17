use std::path::Path;

use eframe::egui::{self, RichText};

use crate::theme;
use crate::ui::layout::card;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepState {
    Waiting,
    Running,
    Passed,
    Failed,
}

#[derive(Clone, Debug)]
struct GateStep {
    id: &'static str,
    label: &'static str,
    state: StepState,
}

const STEPS: [(&str, &str); 5] = [
    ("fmt", "Format"),
    ("check", "Cargo Check"),
    ("clippy", "Clippy"),
    ("tests", "Workspace Tests"),
    ("content", "Content Validation"),
];

pub fn show(ui: &mut egui::Ui, log_path: &Path) {
    let text = std::fs::read_to_string(log_path).unwrap_or_default();
    let steps = parse(&text);

    card(ui, "Quality Gate Pipeline", |ui| {
        ui.colored_label(
            theme::muted(),
            "Live progress from scripts/check.ps1. The detailed output stays in Logs → Quality Gate.",
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (index, step) in steps.iter().enumerate() {
                if index > 0 {
                    ui.colored_label(theme::muted(), ">");
                }
                let (status, color) = match step.state {
                    StepState::Waiting => ("WAIT", theme::muted()),
                    StepState::Running => ("RUN", theme::accent()),
                    StepState::Passed => ("PASS", theme::success()),
                    StepState::Failed => ("FAIL", theme::destructive()),
                };
                ui.label(
                    RichText::new(format!("{status} {}", step.label))
                        .color(color)
                        .strong(),
                );
            }
        });
    });
}

pub fn failed_step(log_path: &Path) -> Option<&'static str> {
    let text = std::fs::read_to_string(log_path).ok()?;
    parse(&text)
        .into_iter()
        .find(|step| step.state == StepState::Failed)
        .map(|step| step.label)
}

fn parse(text: &str) -> Vec<GateStep> {
    let mut steps: Vec<GateStep> = STEPS
        .iter()
        .map(|(id, label)| GateStep {
            id,
            label,
            state: StepState::Waiting,
        })
        .collect();

    for line in text.lines() {
        let clean = line.trim();
        let mut parts = clean.split('|');
        if parts.next() != Some("HUB_GATE") {
            continue;
        }
        let Some(event) = parts.next() else {
            continue;
        };
        let Some(id) = parts.next() else {
            continue;
        };
        let Some(step) = steps.iter_mut().find(|step| step.id == id) else {
            continue;
        };
        step.state = match event {
            "START" => StepState::Running,
            "PASS" => StepState::Passed,
            "FAIL" => StepState::Failed,
            _ => step.state,
        };
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gate_markers_without_reading_human_output() {
        let steps = parse(
            "HUB_GATE|START|fmt|Format\nHUB_GATE|PASS|fmt|Format\nHUB_GATE|START|check|Cargo Check\n",
        );
        assert_eq!(steps[0].state, StepState::Passed);
        assert_eq!(steps[1].state, StepState::Running);
        assert_eq!(steps[2].state, StepState::Waiting);
    }

    #[test]
    fn failed_step_reports_first_failed_pipeline_stage() {
        let steps = parse(
            "HUB_GATE|PASS|fmt|Format\nHUB_GATE|PASS|check|Cargo Check\nHUB_GATE|FAIL|clippy|Clippy\n",
        );
        assert_eq!(
            steps
                .into_iter()
                .find(|step| step.state == StepState::Failed)
                .map(|step| step.label),
            Some("Clippy")
        );
    }
}
