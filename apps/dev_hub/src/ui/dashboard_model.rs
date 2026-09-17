//! Dashboard presentation models — shape runtime snapshot for drawing.
//! Runtime remains authoritative; this layer does not invent lifecycle truth.

use eframe::egui::Color32;
use purgatory_dev_runtime::{CheckStatus, HubSnapshot, ListenerDiag, ServerState, ValidationState};

use crate::theme;

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
    pub metrics: Vec<MetricVm>,
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
    pub git_stamp: String,
    pub workspace_short: String,
    pub workspace_full: String,
    pub identity_full: String,
}

#[derive(Clone, Debug)]
pub struct DashVm {
    pub server: StatusModuleVm,
    pub project: ProjectVm,
    pub attention: AttentionVm,
    pub cargo_warning: Option<&'static str>,
}

pub fn from_snapshot(snap: &HubSnapshot) -> DashVm {
    DashVm {
        server: server_vm(snap),
        project: project_vm(snap),
        attention: attention_vm(snap),
        cargo_warning: (!snap.cargo_found).then_some("cargo not on PATH"),
    }
}

fn server_vm(snap: &HubSnapshot) -> StatusModuleVm {
    let state = snap.server_state;
    let badge = state.as_str().to_ascii_uppercase();
    let badge_color = theme::state_color(state);
    let mut metrics = Vec::new();

    let (headline, detail) = match state {
        ServerState::Stopped => ("Server is not running.".to_string(), None),
        ServerState::Failed => ("Server failed.".to_string(), snap.last_failure.clone()),
        ServerState::Building => ("Building server binary…".to_string(), None),
        ServerState::Starting => ("Starting server process…".to_string(), None),
        ServerState::Verifying => (
            "Verifying readiness…".to_string(),
            (!snap.connection_reason.is_empty()).then(|| snap.connection_reason.clone()),
        ),
        ServerState::Stopping => ("Stopping server…".to_string(), None),
        ServerState::Ready => ("Server is ready.".to_string(), None),
        ServerState::Degraded => ("Server is degraded.".to_string(), snap.last_failure.clone()),
    };

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
    if matches!(state, ServerState::Ready | ServerState::Degraded) {
        metrics.push(MetricVm {
            key: "Health",
            value: health_label(snap.health),
            mono: false,
        });
    }

    StatusModuleVm {
        icon: "S",
        title: "Server",
        badge,
        badge_color,
        headline,
        detail,
        metrics,
    }
}

fn health_label(h: CheckStatus) -> String {
    match h {
        CheckStatus::Pass => "Healthy".into(),
        CheckStatus::Fail => "Unhealthy".into(),
        CheckStatus::Unknown => "Unknown".into(),
    }
}

fn project_vm(snap: &HubSnapshot) -> ProjectVm {
    ProjectVm {
        git_stamp: short_build(&snap.identity),
        workspace_short: shorten_path(&snap.workspace, 58),
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

    if snap.server_state == ServerState::Degraded && snap.last_failure.is_none() {
        issues.push((
            "Server degraded".into(),
            theme::state_color(ServerState::Degraded),
        ));
    }

    if snap.process_alive && snap.listener == ListenerDiag::No {
        issues.push((
            format!(
                "Server process is alive but {} is not listening",
                snap.endpoint
            ),
            theme::destructive(),
        ));
    }

    if snap.server_state.uses_live_process() && snap.connection == CheckStatus::Fail {
        let detail = if snap.connection_reason.is_empty() {
            "Server readiness connection probe failed".to_owned()
        } else {
            format!("Server probe: {}", truncate(&snap.connection_reason, 88))
        };
        issues.push((detail, theme::destructive()));
    }

    if matches!(
        snap.server_state,
        ServerState::Ready | ServerState::Degraded
    ) && snap.health == CheckStatus::Fail
    {
        issues.push((
            "Server health/metrics check failed".into(),
            theme::destructive(),
        ));
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

    if !snap.cargo_found {
        issues.push((
            "cargo not on PATH".into(),
            theme::state_color(ServerState::Degraded),
        ));
    }

    AttentionVm { issues }
}

fn short_build(identity: &str) -> String {
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
    let tail: String = path.chars().rev().take(max.saturating_sub(2)).collect();
    format!("…{}", tail.chars().rev().collect::<String>())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}
