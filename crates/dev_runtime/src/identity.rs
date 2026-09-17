use std::path::Path;
use std::process::Command;

use crate::paths::WorkspacePaths;

#[derive(Clone, Debug)]
pub struct CodeIdentity {
    pub version: String,
    pub phase: String,
    pub git: String,
}

impl CodeIdentity {
    pub fn load(paths: &WorkspacePaths) -> Self {
        Self {
            version: workspace_version(&paths.root),
            phase: phase_token(&paths.root),
            git: git_stamp(&paths.root),
        }
    }

    #[must_use]
    pub fn display(&self) -> String {
        let mut s = format!("v{} - Phase {}", self.version, self.phase);
        if !self.git.is_empty() {
            s.push_str(" - ");
            s.push_str(&self.git);
        }
        s
    }
}

fn workspace_version(root: &Path) -> String {
    let Ok(toml) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return "0.0.0".to_string();
    };
    let Some(idx) = toml.find("[workspace.package]") else {
        return "0.0.0".to_string();
    };
    let rest = &toml[idx..];
    for line in rest.lines().skip(1) {
        if line.starts_with('[') {
            break;
        }
        let t = line.trim();
        if let Some(v) = t.strip_prefix("version") {
            let v = v.trim().trim_start_matches('=').trim().trim_matches('"');
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    "0.0.0".to_string()
}

fn phase_token(root: &Path) -> String {
    std::fs::read_to_string(root.join("PHASE"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "?".to_string())
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn git_stamp(root: &Path) -> String {
    let Some(hash) = git_output(root, &["rev-parse", "--short", "HEAD"]) else {
        return String::new();
    };
    let branch = git_output(root, &["branch", "--show-current"])
        .unwrap_or_else(|| "detached".to_string());
    let dirty = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| !s.trim().is_empty());
    let dirty_suffix = if dirty { "*" } else { "" };
    format!("{branch} @ {hash}{dirty_suffix}")
}
