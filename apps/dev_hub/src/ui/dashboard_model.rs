//! Dashboard presentation models — shape runtime snapshot for drawing.
//! Runtime remains authoritative; this layer does not invent lifecycle truth.

use eframe::egui::Color32;
use purgatory_dev_runtime::{HubCommand, HubSnapshot, ServerState, ValidationState};

use crate::theme;
use crate::ui::status;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerActionKind {
    Restart,
    Stop,
}

impl ServerActionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Restart => "Restart Server",
            Self::Stop => "Stop",
        }
    }

    pub fn command(self) -> HubCommand {
        match self {
            Self::Restart => HubCommand::Restart,
            Self::Stop => HubCommand::Stop,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MetricVm {
    pub key: &'static str,
    pub value: String,
    pub mono: bool,
}

#[derive(Clone, Debug)]
pub struct StatusModuleVm {
    pub icon: &'static str,
    pub title: &'static str,
    pub badge: String,
    pub badge_color: Color32,
    pub headline: String,
    pub detail: Option<String>,
    /// Always-visible compact metrics.
    pub metrics: Vec<MetricVm>,
    /// Extra fields behind a Details disclosure (Server card).
    pub detail_metrics: Vec<MetricVm>,
    pub primary_action: Option<(&'static str, HubCommand)>,
    pub secondary_actions: Vec<(ServerActionKind, bool)>,
    pub nav_label: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct AttentionVm {
    pub issues: Vec<(String, Color32)>,
}

impl AttentionVm {
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct ProjectVm {
    pub phase: String,
    pub profile: String,
    pub job: String,
    pub clients: String,
    pub build: String,
    pub workspace_short: String,
    pub workspace_full: String,
    pub identity_full: String,
}

#[derive(Clone, Debug)]
pub struct QuickActionVm {
    pub label: &'static str,
    pub command: HubCommand,
    pub enabled: bool,
    pub kind: ActionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Routine,
    Secondary,
    Destructive,
}

#[derive(Clone, Debug)]
pub struct DashVm {
    pub server: StatusModuleVm,
    pub validation: StatusModuleVm,
    pub latest: StatusModuleVm,
    pub project: ProjectVm,
    pub attention: AttentionVm,
    pub actions: Vec<QuickActionVm>,
    pub activity: Vec<String>,
    pub cargo_warning: Option<&'static str>,
}

pub fn from_snapshot(snap: &HubSnapshot) -> DashVm {
    DashVm {
        server: server_vm(snap),
        validation: validation_vm(snap),
        latest: latest_vm(snap),
        project: project_vm(snap),
        attention: attention_vm(snap),
        actions: actions_vm(snap),
        activity: snap.log_lines.iter().rev().take(6).rev().cloned().collect(),
        cargo_warning: (!snap.cargo_found).then_some("cargo not on PATH"),
    }
}

fn server_vm(snap: &HubSnapshot) -> StatusModuleVm {
    let state = snap.server_state;
    let badge = state.as_str().to_ascii_uppercase();
    let badge_color = theme::state_color(state);
    let mut metrics = Vec::new();
    let headline;
    let detail;

    // Full field set always available under Details (— when N/A).
    let detail_metrics = server_detail_metrics(snap);

    match state {
        ServerState::Stopped => {
            headline = "Server is not running.".to_string();
            detail = None;
            metrics.push(MetricVm {
                key: "Endpoint",
                value: snap.endpoint.clone(),
                mono: true,
            });
        }
        ServerState::Failed => {
            headline = "Server failed.".to_string();
            detail = snap.last_failure.clone();
            metrics.push(MetricVm {
                key: "Endpoint",
                value: snap.endpoint.clone(),
                mono: true,
            });
        }
        ServerState::Building | ServerState::Starting | ServerState::Verifying => {
            headline = match state {
                ServerState::Building => "Building server binary…".into(),
                ServerState::Starting => "Starting server process…".into(),
                _ => "Verifying readiness…".into(),
            };
            detail = None;
            metrics.push(MetricVm {
                key: "Endpoint",
                value: snap.endpoint.clone(),
                mono: true,
            });
            if let Some(pid) = snap.pid {
                metrics.push(MetricVm {
                    key: "PID",
                    value: pid.to_string(),
                    mono: true,
                });
            }
        }
        ServerState::Stopping => {
            headline = "Stopping server…".to_string();
            detail = None;
            metrics.push(MetricVm {
                key: "Endpoint",
                value: snap.endpoint.clone(),
                mono: true,
            });
        }
        ServerState::Ready | ServerState::Degraded => {
            headline = if state == ServerState::Degraded {
                "Server is degraded.".into()
            } else {
                "Server is ready.".into()
            };
            detail = snap.last_failure.clone();
            if let Some(pid) = snap.pid {
                metrics.push(MetricVm {
                    key: "PID",
                    value: pid.to_string(),
                    mono: true,
                });
            }
            if let Some(origin) = snap.process_origin {
                metrics.push(MetricVm {
                    key: "Origin",
                    value: origin.as_str().to_string(),
                    mono: false,
                });
            }
            metrics.push(MetricVm {
                key: "Endpoint",
                value: snap.endpoint.clone(),
                mono: true,
            });
            metrics.push(MetricVm {
                key: "Health",
                value: health_label(snap.health),
                mono: false,
            });
        }
    }

    let mut secondary = Vec::new();
    let primary = match state {
        ServerState::Stopped | ServerState::Failed => {
            if snap.can_start {
                Some(("Start Server", HubCommand::Start))
            } else {
                None
            }
        }
        ServerState::Ready | ServerState::Degraded => {
            if snap.can_restart {
                secondary.push((ServerActionKind::Restart, true));
            }
            if snap.can_stop {
                secondary.push((ServerActionKind::Stop, true));
            }
            None
        }
        ServerState::Building
        | ServerState::Starting
        | ServerState::Verifying
        | ServerState::Stopping => {
            if snap.can_stop {
                secondary.push((ServerActionKind::Stop, true));
            }
            None
        }
    };

    StatusModuleVm {
        icon: "▣",
        title: "Server",
        badge,
        badge_color,
        headline,
        detail,
        metrics,
        detail_metrics,
        primary_action: primary,
        secondary_actions: secondary,
        nav_label: Some("Open Server"),
    }
}

fn server_detail_metrics(snap: &HubSnapshot) -> Vec<MetricVm> {
    use purgatory_dev_runtime::ProcessOrigin;
    vec![
        MetricVm {
            key: "Endpoint",
            value: snap.endpoint.clone(),
            mono: true,
        },
        MetricVm {
            key: "PID",
            value: snap
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "—".into()),
            mono: true,
        },
        MetricVm {
            key: "Origin",
            value: snap
                .process_origin
                .map(ProcessOrigin::as_str)
                .unwrap_or("—")
                .to_string(),
            mono: false,
        },
        MetricVm {
            key: "Health",
            value: health_label(snap.health),
            mono: false,
        },
        MetricVm {
            key: "Probe",
            value: snap.connection.as_ui().to_string(),
            mono: false,
        },
        MetricVm {
            key: "Listener",
            value: snap.listener.as_ui().to_string(),
            mono: false,
        },
        MetricVm {
            key: "Alive",
            value: if snap.process_alive {
                "yes".into()
            } else {
                "no".into()
            },
            mono: false,
        },
        MetricVm {
            key: "Job",
            value: status::job_label(snap.job),
            mono: false,
        },
    ]
}

fn health_label(h: purgatory_dev_runtime::CheckStatus) -> String {
    match h {
        purgatory_dev_runtime::CheckStatus::Pass => "Healthy".into(),
        purgatory_dev_runtime::CheckStatus::Fail => "Unhealthy".into(),
        purgatory_dev_runtime::CheckStatus::Unknown => "Unknown".into(),
    }
}

fn validation_vm(snap: &HubSnapshot) -> StatusModuleVm {
    let phase = snap.validation;
    let badge = phase.as_str().to_ascii_uppercase();
    let badge_color = theme::outcome_color(phase);
    let mut metrics = Vec::new();
    let headline;
    let detail;

    if phase.is_active() {
        headline = snap
            .validation_active_label
            .clone()
            .unwrap_or_else(|| "Validation in progress…".into());
        if let Some(elapsed) = snap.validation_live.elapsed_secs {
            metrics.push(MetricVm {
                key: "Elapsed",
                value: crate::ui::layout::format_secs(elapsed),
                mono: true,
            });
        }
        detail = snap.validation_live.status_line.clone();
    } else if phase == ValidationState::Idle {
        headline = "Validation available.".into();
        detail = Some("No run in progress.".into());
    } else {
        headline = format!("Last phase: {}", phase.as_str());
        detail = snap.validation_reason.clone();
    }

    // Dashboard does not invent ValidationSpec — navigate to Validation page.
    StatusModuleVm {
        icon: "⛨",
        title: "Validation",
        badge,
        badge_color,
        headline,
        detail,
        metrics,
        detail_metrics: Vec::new(),
        primary_action: None,
        secondary_actions: Vec::new(),
        nav_label: Some("Open Validation"),
    }
}

fn latest_vm(snap: &HubSnapshot) -> StatusModuleVm {
    let outcome = snap.validation_last.outcome.or(snap.load_last.outcome);
    if let Some(outcome) = outcome {
        let mut metrics = Vec::new();
        let summary = if snap.validation_last.summary.available {
            Some(&snap.validation_last.summary)
        } else if snap.load_last.summary.available {
            Some(&snap.load_last.summary)
        } else {
            None
        };
        if let Some(s) = summary {
            if !s.run_status.is_empty() {
                metrics.push(MetricVm {
                    key: "Status",
                    value: s.run_status.clone(),
                    mono: false,
                });
            }
            metrics.push(MetricVm {
                key: "Peak",
                value: format!("{}/{}", s.peak_connected, s.requested_bots),
                mono: true,
            });
            if s.elapsed_secs.is_finite() && s.elapsed_secs > 0.0 {
                metrics.push(MetricVm {
                    key: "Elapsed",
                    value: crate::ui::layout::format_secs(s.elapsed_secs),
                    mono: true,
                });
            }
        }
        StatusModuleVm {
            icon: "▥",
            title: "Latest Result",
            badge: outcome.as_str().to_ascii_uppercase(),
            badge_color: theme::outcome_color(outcome),
            headline: format!("Completed: {}", outcome.as_str()),
            detail: None,
            metrics,
            detail_metrics: Vec::new(),
            primary_action: None,
            secondary_actions: Vec::new(),
            nav_label: Some("Open Validation"),
        }
    } else {
        StatusModuleVm {
            icon: "▥",
            title: "Latest Result",
            badge: String::new(),
            badge_color: theme::muted(),
            headline: "No completed validation run yet.".into(),
            detail: Some("Run validation to establish a baseline.".into()),
            metrics: Vec::new(),
            detail_metrics: Vec::new(),
            primary_action: None,
            secondary_actions: Vec::new(),
            nav_label: Some("Open Validation"),
        }
    }
}

fn project_vm(snap: &HubSnapshot) -> ProjectVm {
    let build = short_build(&snap.identity);
    let workspace_short = shorten_path(&snap.workspace, 48);
    ProjectVm {
        phase: snap.phase.clone(),
        profile: snap.build_profile.clone(),
        job: status::job_label(snap.job),
        clients: if snap.pending_clients > 0 {
            format!("{} (+{})", snap.client_count, snap.pending_clients)
        } else {
            snap.client_count.to_string()
        },
        build,
        workspace_short,
        workspace_full: snap.workspace.clone(),
        identity_full: snap.identity.clone(),
    }
}

fn attention_vm(snap: &HubSnapshot) -> AttentionVm {
    let mut issues = Vec::new();
    if let Some(fail) = &snap.last_failure
        && matches!(
            snap.server_state,
            ServerState::Failed | ServerState::Degraded
        )
    {
        issues.push((truncate(fail, 96), theme::state_color(ServerState::Failed)));
    }
    if matches!(
        snap.validation,
        ValidationState::Failed | ValidationState::OrchestrationFailed
    ) {
        issues.push((
            format!("Validation: {}", snap.validation.as_str()),
            theme::outcome_color(snap.validation),
        ));
    }
    if matches!(
        snap.load,
        ValidationState::Failed | ValidationState::OrchestrationFailed
    ) {
        issues.push((
            format!("Load: {}", snap.load.as_str()),
            theme::outcome_color(snap.load),
        ));
    }
    if snap.server_state == ServerState::Degraded && snap.last_failure.is_none() {
        issues.push((
            "Server degraded".into(),
            theme::state_color(ServerState::Degraded),
        ));
    }
    if !snap.cargo_found {
        issues.push((
            "cargo not on PATH".into(),
            theme::state_color(ServerState::Degraded),
        ));
    }
    AttentionVm { issues }
}

fn actions_vm(snap: &HubSnapshot) -> Vec<QuickActionVm> {
    vec![
        QuickActionVm {
            label: "+ 1 Client",
            command: HubCommand::RequestClients { count: 1 },
            enabled: snap.can_request_clients,
            kind: ActionKind::Routine,
        },
        QuickActionVm {
            label: "Quality Gate",
            command: HubCommand::QualityGate,
            enabled: true,
            kind: ActionKind::Secondary,
        },
        QuickActionVm {
            label: "Rebuild",
            command: HubCommand::Rebuild,
            enabled: true,
            kind: ActionKind::Secondary,
        },
        QuickActionVm {
            label: "Kill All",
            command: HubCommand::KillAll,
            enabled: true,
            kind: ActionKind::Destructive,
        },
    ]
}

fn short_build(identity: &str) -> String {
    // identity: "v0.x.y - Phase 6G - abcdef*"
    identity
        .rsplit(" - ")
        .next()
        .unwrap_or(identity)
        .trim()
        .to_string()
}

fn shorten_path(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_string();
    }
    let chars: Vec<char> = path.chars().collect();
    let keep = max.saturating_sub(1);
    let start = chars.len().saturating_sub(keep);
    format!("…{}", chars[start..].iter().collect::<String>())
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_build_takes_trailing_stamp() {
        assert_eq!(short_build("v0.1.0 - Phase 6G - 077fa0c*"), "077fa0c*");
    }

    #[test]
    fn shorten_path_preserves_tail() {
        let p = "C:\\Users\\Ariel\\OneDrive\\Desktop\\Purgatory\\1";
        let s = shorten_path(p, 20);
        assert!(s.starts_with('…'));
        assert!(s.ends_with("Purgatory\\1") || s.ends_with("1"));
    }
}
